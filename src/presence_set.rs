//! 多子元素声明式进出容器。
//!
//! [`PresenceSet`] 是 [`crate::PresenceState`] 的多 key 泛化：按
//! [`SharedString`] key 管理任意数量的 child，每个 key 拥有独立、互不干扰的
//! 进出场生命周期。典型场景：列表项增删、标签页切换、通知栈。
//!
//! [`crate::PresenceState`] 可视为单 key（固定 id）的 [`PresenceSet`] 特例；
//! [`PresenceSet`] 在共享同一 [`MotionLifecycle`] 与同一 builder 的前提下，
//! 将单 child 状态机复制为每 key 一份（[`Entry`]）。
//!
//! # 状态机
//!
//! 每个 key 独立沿用 [`crate::PresenceState`] 的 S5 / S6 / S7 语义：
//!
//! - **S5 epoch 守卫**：每次入场 / 退场递增该 key 的 `epoch`；退场定时器 spawn 前
//!   捕获 epoch，回调仅当 `closing && epoch` 匹配时才生效（退场中途重开 →
//!   旧定时器过期事件被忽略，无副作用）。
//! - **S6 转换快照**：状态转换瞬间捕获入场 / 退场动效与预构建
//!   `Rc<Animation>`，进行中的过渡不受后续 [`PresenceSet::set_lifecycle`] 影响。
//! - **S7 卸载宽限**：退场定时器 = 退场时长 + delay + `EXIT_TIMER_GRACE`（50ms），
//!   兜底 GPUI 动画起点在首次 layout 才打点的滞后；退场完成后条目自动从容器移除。
//!
//! 每 key 不变式：`closing ⟹ visible`；`exit_active.is_some() ⟺ closing`；
//! `enter_active.is_some() ⟺ visible ∧ ¬closing`（render 中的防御性
//! `unwrap_or` 允许瞬时例外）。
//!
//! # builder 约定
//!
//! builder 以 `(&key, window, cx)` 为参数，每帧被调用以按 key 重建 child 元素；
//! 闭包应捕获 `WeakEntity` 以读取最新状态，避免循环引用（与
//! [`crate::PresenceState::new`] 的 builder 约定一致）。

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Animation, App, AppContext, Context, Div, ElementId, Entity, IntoElement, ParentElement,
    Render, SharedString, Styled, Task, Window, div,
};

use crate::animated::build_animation;
use crate::{Animated, Motion, MotionLifecycle};

/// 退场卸载定时器宽限期：动画结束后额外等待一段时间再卸载，
/// 兜底 GPUI 在元素首次 layout 才打点 `AnimationState.start`（至多滞后一帧）。
/// 语义与 [`crate::PresenceState`] 的 50ms 宽限完全一致。
const EXIT_TIMER_GRACE: Duration = Duration::from_millis(50);

/// child 构造闭包：每帧被调用以按 key 重建元素。
///
/// 使用 `Rc<dyn Fn>` 而非 `FnOnce`，因为退场期间需要重复构造 child。
/// 闭包应捕获 `WeakEntity` 以读取最新状态，避免循环引用。
/// 返回 `Div` 而非 `AnyElement`，因为 [`Animated<T>`] 要求 `T: Styled`，
/// 而 `AnyElement` 未实现 `Styled`。`Div` 是 GPUI 通用容器，可包裹任意子元素。
type EntryBuilder = Rc<dyn Fn(&SharedString, &mut Window, &mut App) -> Div>;

/// 单个 key 的生命周期状态（镜像 [`crate::PresenceState`] 的私有状态）。
struct Entry {
    /// 实际渲染状态（退场动画结束后才变 `false`，随后条目被移除）。
    visible: bool,
    /// 退场动画进行中。
    closing: bool,
    /// ID 计数器，每次入场递增，编入 `ElementId` 保证唯一。
    epoch: usize,
    /// 进行中过渡的入场快照（转换发生时捕获，不受后续 `set_lifecycle` 影响）。
    /// 预构建 `Rc<Animation>`（P1 缓存），渲染期直接复用（P2）。
    enter_active: Option<(Motion, Rc<Animation>)>,
    /// 进行中过渡的退场快照。
    exit_active: Option<(Motion, Rc<Animation>)>,
    /// 退场计时器，存储以便中断时取消。
    exit_task: Option<Task<()>>,
}

/// 多子元素声明式进出容器。
///
/// 通过 [`PresenceSet::new`] 创建一个 `Entity<PresenceSet>`，
/// 每帧调用 [`PresenceSet::set_present`] 声明每个 key 的期望状态，
/// 框架按 key 自动播入场 / 退场动画；退场完成后条目自动移除。
///
/// # 示例
///
/// ```ignore
/// use gpui_component_motion::{AnimationSpec, MotionLifecycle, PresenceSet};
/// use gpui::{div, px};
///
/// // 在 App::new 中创建（一次）：builder 捕获 WeakEntity 读取最新状态
/// let weak = cx.entity().downgrade();
/// let set = PresenceSet::new(
///     cx,
///     MotionLifecycle::slide_up(px(8.), AnimationSpec::default()),
///     move |key, window, cx| {
///         if let Some(strong) = weak.upgrade() {
///             strong.read(cx).render_item(key, window, cx)
///         } else {
///             div()
///         }
///     },
/// );
///
/// // 每帧更新 + 插入
/// self.set.update(cx, |s, cx| {
///     s.set_present("tab-1", self.tab_1_open, window, cx);
///     s.set_present("tab-2", self.tab_2_open, window, cx);
/// });
/// row.child(self.set.clone());
/// ```
pub struct PresenceSet {
    /// 入场 / 退场动画配对（所有 key 共享）。
    lifecycle: MotionLifecycle,
    /// child 构造闭包（按 key 重建）。
    builder: EntryBuilder,
    /// 条目列表：保持插入序，查找用线性 find（条目数小）。
    entries: Vec<(SharedString, Entry)>,
}

impl PresenceSet {
    /// 创建 `PresenceSet` 实体。
    ///
    /// `builder` 闭包在每帧被调用，以 `(&key, window, cx)` 为参数按 key 重建
    /// child 元素。闭包应捕获 `WeakEntity` 以读取最新状态，避免循环引用
    /// （同 [`crate::PresenceState::new`] 的 builder 约定）。
    pub fn new<F>(cx: &mut App, lifecycle: MotionLifecycle, builder: F) -> Entity<Self>
    where
        F: Fn(&SharedString, &mut Window, &mut App) -> Div + 'static,
    {
        cx.new(|_cx| Self {
            lifecycle,
            builder: Rc::new(builder),
            entries: Vec::new(),
        })
    }

    /// 更新指定 key 的期望存在状态。
    ///
    /// - `true`：key 应存在（无条目则插入并入场；退场中则中断退场重新入场，
    ///   `epoch` 总是递增，以保证入场动画 ID 与历史所有动画不同，避免 GPUI
    ///   `AnimationState` 缓存命中 `delta=1.0` 导致的"无动画瞬现"）。
    /// - `false`：key 应卸载（先播退场动画，结束后条目自动从容器移除）。
    /// - 与当前期望状态相同（已可见稳定态 / 无此条目 / 已在退场中）时直接返回（短路）。
    ///
    /// 状态机语义与 [`crate::PresenceState::set_present`] 逐条对齐：S5 epoch 守卫
    /// （定时器回调仅当 `closing && epoch` 匹配时生效）、S6 转换快照（进行中的
    /// 过渡不受后续 [`set_lifecycle`](Self::set_lifecycle) 影响）、S7 卸载宽限
    /// （定时器 = 退场时长 + delay + `EXIT_TIMER_GRACE`）。
    pub fn set_present(
        &mut self,
        key: impl Into<SharedString>,
        present: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = key.into();
        let idx = self.entries.iter().position(|(k, _)| *k == key);

        if present {
            // 短路：已存在且处于可见稳定态（visible && !closing），即目标已为 true。
            if let Some(i) = idx {
                let entry = &self.entries[i].1;
                if entry.visible && !entry.closing {
                    return;
                }
            }
            // 无条目则插入（visible=false 初始），或从 closing 恢复。
            let i = match idx {
                Some(i) => i,
                None => {
                    self.entries.push((
                        key.clone(),
                        Entry {
                            visible: false,
                            closing: false,
                            epoch: 0,
                            enter_active: None,
                            exit_active: None,
                            exit_task: None,
                        },
                    ));
                    self.entries.len() - 1
                }
            };
            let entry = &mut self.entries[i].1;
            // 进入入场态：无论从 HIDDEN（visible=false）还是从中断 EXITING
            // （closing=true, visible=true）进入，都必须递增 epoch——后者是最易被
            // 忽略的 bug 来源：若 epoch 不变，入场 ID（epoch*2 偶数）会与关闭前
            // 那次入场的 ID 完全相同 → GPUI 缓存的 AnimationState 已是 delta=1.0
            // 的完成态 → 条目"哐当"一下以全态出现无动画。
            entry.epoch += 1;
            // 捕获本次入场的快照（预构建 `Rc<Animation>`，P2），
            // 进行中的过渡不受 set_lifecycle 影响（S6）。
            entry.enter_active = Some((
                self.lifecycle.enter,
                build_animation(self.lifecycle.enter_spec, false),
            ));
            entry.exit_active = None;
            if entry.closing {
                entry.closing = false;
                // 丢弃任务即取消定时器（C8）。
                entry.exit_task = None;
            }
            entry.visible = true;
        } else {
            // 短路：无条目，或已在退场中（目标已为 false）。
            let i = match idx {
                Some(i) if self.entries[i].1.visible && !self.entries[i].1.closing => i,
                _ => return,
            };
            let entry = &mut self.entries[i].1;
            // 启动退场：递增 epoch 保证退场 ID（epoch*2+1 奇数）
            // 不与任何历史退场/入场 ID 冲突。
            entry.epoch += 1;
            // 捕获本次退场的快照（预构建 `Rc<Animation>`，reverse=true，P2）。
            entry.enter_active = None;
            entry.exit_active = Some((
                self.lifecycle.exit,
                build_animation(self.lifecycle.exit_spec, true),
            ));
            entry.closing = true;
            // S7 宽限：定时器 = 快照退场时长 + delay + GRACE（兜底 C5 首帧滞后）。
            let exit_spec = self.lifecycle.exit_spec;
            let exit_duration = exit_spec.duration + exit_spec.delay + EXIT_TIMER_GRACE;
            // S5 epoch 守卫：spawn 前捕获，回调仅当 epoch 匹配且仍处 closing 时生效。
            let epoch = entry.epoch;
            entry.exit_task = Some(cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(exit_duration).await;
                _ = this.update_in(cx, |s, _window, cx| {
                    // 用索引先收集再 remove：匹配 key 且 epoch 守卫通过才卸载。
                    let mut to_remove = Vec::new();
                    for (i, (entry_key, entry)) in s.entries.iter_mut().enumerate() {
                        if *entry_key == key && entry.closing && entry.epoch == epoch {
                            entry.closing = false;
                            entry.visible = false;
                            entry.enter_active = None;
                            entry.exit_active = None;
                            entry.exit_task = None;
                            to_remove.push(i);
                        }
                    }
                    for i in to_remove.into_iter().rev() {
                        s.entries.remove(i);
                    }
                    cx.notify();
                });
            }));
        }
        cx.notify();
    }

    /// 更新动画配对（运行时切换缓动模式等）。
    ///
    /// 进行中的过渡不受影响（读转换时捕获的快照），新 lifecycle 仅对后续过渡生效。
    /// 与当前 lifecycle 相等时直接返回（不通知、不触碰快照）。语义与
    /// [`crate::PresenceState::set_lifecycle`] 一致。
    pub fn set_lifecycle(&mut self, lifecycle: MotionLifecycle, cx: &mut Context<Self>) {
        if lifecycle == self.lifecycle {
            return;
        }
        self.lifecycle = lifecycle;
        cx.notify();
    }

    /// key 是否处于"存在"状态（等价于最近一次声明，即 `target` 语义）：
    /// 条目存在、可见且未在退场中。
    ///
    /// 退场动画期间（`closing`）返回 `false`，即使元素仍在屏幕上播退场动画。
    pub fn is_present(&self, key: &str) -> bool {
        self.entries
            .iter()
            .any(|(k, e)| k.as_ref() == key && e.visible && !e.closing)
    }

    /// 条目总数（含退场动画尚未结束、仍待移除的条目）。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否没有任何条目（含退场中条目）。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Render for PresenceSet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 按插入序遍历，仅渲染可见条目（含退场动画进行中的条目，closing ⟹ visible）。
        let mut elements = Vec::new();
        for (key, entry) in &self.entries {
            if !entry.visible {
                continue;
            }
            // builder 按 key 重建 child；外层 overflow_hidden + flex_shrink_0
            // 与 PresenceState 同款安全默认（动画容器在父 flex 中不被压缩）。
            let child = (self.builder)(key, window, cx);
            let child = div().overflow_hidden().flex_shrink_0().child(child);
            let id_base = key.clone();
            let epoch = entry.epoch;

            if entry.closing {
                // 退场：奇数 ID（epoch*2+1）避免与入场动画 ID 冲突
                // （GPUI 会跳过已完成动画）。优先读退场快照（预构建 `Rc<Animation>`，
                // P2 直接复用）；`unwrap_or` 兜底仅作防御性路径。
                let id = ElementId::NamedInteger(id_base, (epoch * 2 + 1) as u64);
                let (motion, animation) = match &entry.exit_active {
                    Some((motion, animation)) => (*motion, animation.clone()),
                    None => (
                        self.lifecycle.exit,
                        build_animation(self.lifecycle.exit_spec, true),
                    ),
                };
                elements.push(
                    Animated::from_animation(
                        child,
                        id,
                        motion,
                        self.lifecycle.exit_spec,
                        animation,
                        true,
                    )
                    .into_any_element(),
                );
            } else {
                // 入场：偶数 ID（epoch*2），每次重开都是新 ID（key 作基名保证跨 key 唯一）。
                let id = ElementId::NamedInteger(id_base, (epoch * 2) as u64);
                let (motion, animation) = match &entry.enter_active {
                    Some((motion, animation)) => (*motion, animation.clone()),
                    None => (
                        self.lifecycle.enter,
                        build_animation(self.lifecycle.enter_spec, false),
                    ),
                };
                elements.push(
                    Animated::from_animation(
                        child,
                        id,
                        motion,
                        self.lifecycle.enter_spec,
                        animation,
                        false,
                    )
                    .into_any_element(),
                );
            }
        }
        // 无可见条目：返回空容器（不占位）。
        div().children(elements)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AnimationSpec;
    use gpui::{Empty, TestAppContext};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// 在窗口创建闭包内构造 `PresenceSet`（窗口上下文创建 → 种子化
    /// `current_window`），使退场定时器回调中的 `update_in` 可解析窗口。
    fn setup(
        cx: &mut TestAppContext,
        lifecycle: MotionLifecycle,
    ) -> (gpui::WindowHandle<Empty>, Entity<PresenceSet>) {
        let set_cell: Rc<RefCell<Option<Entity<PresenceSet>>>> = Default::default();
        let window = cx.add_window(|_window, cx| {
            *set_cell.borrow_mut() = Some(PresenceSet::new(cx, lifecycle, |_key, _window, _cx| {
                div().child("content")
            }));
            Empty
        });
        let set = set_cell.borrow().clone().expect("presence_set created");
        (window, set)
    }

    fn set_present(
        cx: &mut TestAppContext,
        set: &Entity<PresenceSet>,
        window: &gpui::WindowHandle<Empty>,
        key: &str,
        present: bool,
    ) {
        cx.update_window(**window, |_, window, cx| {
            set.update(cx, |s, cx| s.set_present(key, present, window, cx));
        })
        .unwrap();
    }

    fn is_present(cx: &TestAppContext, set: &Entity<PresenceSet>, key: &str) -> bool {
        cx.update(|cx| set.read(cx).is_present(key))
    }

    fn len(cx: &TestAppContext, set: &Entity<PresenceSet>) -> usize {
        cx.update(|cx| set.read(cx).len())
    }

    fn entry_state(
        cx: &TestAppContext,
        set: &Entity<PresenceSet>,
        key: &str,
    ) -> Option<(bool, bool, bool)> {
        cx.update(|cx| {
            let s = set.read(cx);
            s.entries
                .iter()
                .find(|(k, _)| k.as_ref() == key)
                .map(|(_, e)| (e.visible, e.closing, e.exit_task.is_some()))
        })
    }

    /// 手动驱动一帧 `render`（T19 模式）。
    fn render_frame(
        cx: &mut TestAppContext,
        set: &Entity<PresenceSet>,
        window: &gpui::WindowHandle<Empty>,
    ) {
        cx.update_window(**window, |_, window, cx| {
            set.update(cx, |s, cx| {
                let _ = s.render(window, cx).into_any_element();
            });
        })
        .unwrap();
    }

    /// 两个 key 独立进出：都 present → 都可见；退一个 → 该 key 进入退场（定时器），
    /// 另一个保持可见稳定态；退场完成后仅该 key 被移除。
    #[gpui::test]
    async fn two_keys_enter_and_exit_independently(cx: &mut TestAppContext) {
        let (window, set) = setup(cx, MotionLifecycle::fade(AnimationSpec::default()));

        // 两个 key 都 present → 都可见
        set_present(cx, &set, &window, "a", true);
        set_present(cx, &set, &window, "b", true);
        assert!(is_present(cx, &set, "a"));
        assert!(is_present(cx, &set, "b"));
        assert_eq!(len(cx, &set), 2);

        // 退 a：a 进入退场（closing + 定时器），b 保持
        set_present(cx, &set, &window, "a", false);
        let (visible, closing, has_task) = entry_state(cx, &set, "a").expect("a 应存在");
        assert!(visible, "退场动画期间元素仍在屏幕上");
        assert!(closing);
        assert!(has_task);
        let b_state = entry_state(cx, &set, "b").expect("b 应存在");
        assert!(b_state.0 && !b_state.1, "b 应保持可见稳定态");
        assert!(is_present(cx, &set, "b"));
        assert!(!is_present(cx, &set, "a"), "a 已声明退出（target 语义）");

        // 推进定时器截止（200ms + 50ms grace）：a 移除，b 保持
        cx.executor().advance_clock(Duration::from_millis(250));
        cx.run_until_parked();

        assert!(entry_state(cx, &set, "a").is_none(), "退场完成后条目应移除");
        assert_eq!(len(cx, &set), 1);
        assert!(is_present(cx, &set, "b"));
    }

    /// 退场后条目移除：false 后推进 total + GRACE → 条目从容器移除（len 减小），
    /// 双快照随条目一并清空（条目移除前退场快照存在、入场快照为空）。
    #[gpui::test]
    async fn exit_removes_entry_after_grace(cx: &mut TestAppContext) {
        let (window, set) = setup(cx, MotionLifecycle::fade(AnimationSpec::default()));

        set_present(cx, &set, &window, "a", true);
        set_present(cx, &set, &window, "a", false);
        assert_eq!(len(cx, &set), 1, "退场中条目仍保留在容器");
        let (visible, closing, _) = entry_state(cx, &set, "a").expect("a 应存在");
        assert!(visible && closing);

        // 快照状态：退场快照存在（reverse=true），入场快照为空
        let (has_enter, has_exit) = cx.update(|cx| {
            let s = set.read(cx);
            let (_, e) = s
                .entries
                .iter()
                .find(|(k, _)| k.as_ref() == "a")
                .expect("a 应存在");
            (e.enter_active.is_some(), e.exit_active.is_some())
        });
        assert!(!has_enter, "退场启动后入场快照应清空");
        assert!(has_exit, "退场应捕获退场快照");

        // 推进 total + GRACE（200ms + 50ms）→ 条目移除
        cx.executor().advance_clock(Duration::from_millis(250));
        cx.run_until_parked();

        assert!(entry_state(cx, &set, "a").is_none(), "条目应被移除");
        assert_eq!(len(cx, &set), 0);
        assert!(cx.update(|cx| set.read(cx).is_empty()));
    }

    /// T11 对齐：退场中途重开——旧定时器过期后必须被忽略（epoch 守卫）。
    #[gpui::test]
    async fn stale_exit_timer_ignored_after_reopen(cx: &mut TestAppContext) {
        let (window, set) = setup(cx, MotionLifecycle::fade(AnimationSpec::default()));

        // true → false：启动退场，定时器 = 200ms + 50ms
        set_present(cx, &set, &window, "a", true);
        set_present(cx, &set, &window, "a", false);

        // 推进半程（100ms）
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();

        // 重开：取消定时器、递增 epoch、重新入场
        set_present(cx, &set, &window, "a", true);
        let (visible, closing, has_task) = entry_state(cx, &set, "a").expect("a 应存在");
        assert!(visible);
        assert!(!closing);
        assert!(!has_task, "重开应取消旧定时器");

        // 推进越过旧定时器截止（再 300ms > 250ms），过期事件必须无副作用
        cx.executor().advance_clock(Duration::from_millis(300));
        cx.run_until_parked();

        let (visible, closing, _) = entry_state(cx, &set, "a").expect("a 应仍存在");
        assert!(visible);
        assert!(!closing);
        assert_eq!(len(cx, &set), 1);
    }

    /// T12 对齐：退场中途 `set_lifecycle`——进行中的退场按转换时快照执行。
    #[gpui::test]
    async fn set_lifecycle_mid_exit_uses_snapshot(cx: &mut TestAppContext) {
        let (window, set) = setup(
            cx,
            MotionLifecycle::fade(
                AnimationSpec::default().with_duration(Duration::from_millis(200)),
            ),
        );

        // 入场并完成
        set_present(cx, &set, &window, "a", true);
        cx.executor().advance_clock(Duration::from_millis(300));
        cx.run_until_parked();

        // 启动退场：定时器 = 200ms + 50ms grace
        set_present(cx, &set, &window, "a", false);

        // 推进半程后切换 lifecycle（退场 500ms）——不影响进行中的退场
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        cx.update(|cx| {
            set.update(cx, |s, cx| {
                s.set_lifecycle(
                    MotionLifecycle::fade(
                        AnimationSpec::default().with_duration(Duration::from_millis(500)),
                    ),
                    cx,
                );
            });
        });

        // 按原 200ms 快照：再推进 150ms 即达 250ms 截止
        cx.executor().advance_clock(Duration::from_millis(150));
        cx.run_until_parked();

        assert!(
            entry_state(cx, &set, "a").is_none(),
            "应按原 200ms 快照卸载（新 lifecycle 不影响进行中过渡）"
        );
        assert!(cx.update(|cx| set.read(cx).is_empty()));
    }

    /// 插入序稳定：entries 保持插入序（render 按插入序遍历）；退场完成的条目移除后
    /// 其余条目相对顺序不变。
    #[gpui::test]
    async fn insert_order_stable(cx: &mut TestAppContext) {
        let (window, set) = setup(cx, MotionLifecycle::fade(AnimationSpec::default()));

        set_present(cx, &set, &window, "first", true);
        set_present(cx, &set, &window, "second", true);
        set_present(cx, &set, &window, "third", true);

        let order = |cx: &TestAppContext| {
            cx.update(|cx| {
                set.read(cx)
                    .entries
                    .iter()
                    .map(|(k, _)| k.to_string())
                    .collect::<Vec<_>>()
            })
        };
        assert_eq!(order(cx), vec!["first", "second", "third"]);
        assert_eq!(len(cx, &set), 3);

        // 渲染驱动：多条目 + 退场中条目均可渲染（不 panic），顺序仍为插入序
        render_frame(cx, &set, &window);
        set_present(cx, &set, &window, "second", false);
        render_frame(cx, &set, &window);
        assert_eq!(order(cx), vec!["first", "second", "third"]);

        // 退场完成：second 移除，其余相对顺序不变
        cx.executor().advance_clock(Duration::from_millis(250));
        cx.run_until_parked();
        assert_eq!(order(cx), vec!["first", "third"]);
    }

    /// builder 按 key 重建：驱动渲染后 builder 应收到各可见条目的 key。
    #[gpui::test]
    async fn builder_receives_key(cx: &mut TestAppContext) {
        let seen: Rc<RefCell<Vec<String>>> = Default::default();
        let seen_cell = seen.clone();
        let set_cell: Rc<RefCell<Option<Entity<PresenceSet>>>> = Default::default();
        let window = cx.add_window(|_window, cx| {
            *set_cell.borrow_mut() = Some(PresenceSet::new(
                cx,
                MotionLifecycle::fade(AnimationSpec::default()),
                move |key, _window, _cx| {
                    seen_cell.borrow_mut().push(key.to_string());
                    div().child("content")
                },
            ));
            Empty
        });
        let set = set_cell.borrow().clone().expect("presence_set created");

        set_present(cx, &set, &window, "alpha", true);
        set_present(cx, &set, &window, "beta", true);

        // 驱动渲染：每帧 builder 应按 key 重建各可见条目
        render_frame(cx, &set, &window);
        render_frame(cx, &set, &window);

        let seen = seen.borrow();
        assert!(
            seen.iter().any(|k| k == "alpha"),
            "builder 应收到 alpha: {seen:?}"
        );
        assert!(
            seen.iter().any(|k| k == "beta"),
            "builder 应收到 beta: {seen:?}"
        );
    }
}
