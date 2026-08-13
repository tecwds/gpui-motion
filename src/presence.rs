//! 声明式生命周期动画容器。
//!
//! [`PresenceState`] 是一个 [`gpui::Render`] 实现，持有一个 child 构造闭包。
//! 调用方只声明"元素何时该存在"（[`PresenceState::set_present`]），
//! 框架自动管理入场 / 退场动画时序与卸载时机。
//!
//! # 状态机
//!
//! ```text
//! HIDDEN ──set_present(true)──▶ ENTERING(epoch+=1, enter_active=快照)
//! ENTERING ──set_present(false)──▶ EXITING(epoch+=1, exit_active=快照, 定时器=total+GRACE)
//! VISIBLE ──set_present(false)──▶ EXITING(同上)
//! EXITING ──set_present(true)──▶ ENTERING(epoch+=1, 取消定时器, exit_active=None)
//! EXITING ──定时器到期且 epoch 匹配──▶ HIDDEN(closing=false, visible=false, 双 active=None)
//! EXITING ──定时器到期但 epoch 不匹配──▶ 忽略（无副作用）
//! ```
//!
//! 不变式：`target` 为期望值；`visible` 为实际挂载；`closing ⟹ visible`；
//! `exit_active.is_some() ⟺ closing`；`enter_active.is_some() ⟺ visible ∧ ¬closing`
//! （render 中的防御性 `unwrap_or` 允许瞬时例外）。
//!
//! 已知限制（S8）：GPUI `Animation` 不支持自定义起始进度，退场→入场 / 入场→退场
//! 打断时存在一次可见跳变。
//!
//! # 定时器宽限的接受边界（S7）
//!
//! - 卡帧 / 窗口 occluded 时元素不渲染，提前卸载不可见（无视觉影响）；
//! - `reduce_motion` 下退场首帧即达隐藏态，此后以隐藏态挂载至定时器到期（无视觉影响）。

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    App, AppContext, Context, Div, ElementId, Entity, IntoElement, ParentElement, Render,
    SharedString, Styled, Task, Window, div, px,
};

use crate::{Animated, AnimationSpec, Motion, MotionLifecycle};

/// 退场卸载定时器宽限期：动画结束后额外等待一段时间再卸载，
/// 兜底 GPUI 在元素首次 layout 才打点 `AnimationState.start`（至多滞后一帧）。
const EXIT_TIMER_GRACE: Duration = Duration::from_millis(50);

/// child 构造闭包：每帧被调用以重建元素。
///
/// 使用 `Rc<dyn Fn>` 而非 `FnOnce`，因为退场期间需要重复构造 child。
/// 闭包应捕获 `WeakEntity` 以读取最新状态，避免循环引用。
///
/// 返回 `Div` 而非 `AnyElement`，因为 [`Animated<T>`] 要求 `T: Styled`，
/// 而 `AnyElement` 未实现 `Styled`。`Div` 是 GPUI 通用容器，可包裹任意子元素。
type ChildBuilder = Rc<dyn Fn(&mut Window, &mut App) -> Div>;

/// 声明式生命周期动画状态。
///
/// 通过 [`PresenceState::new`] 创建一个 `Entity<PresenceState>`，
/// 每帧调用 [`PresenceState::set_present`] 声明期望状态，
/// 框架自动播入场 / 退场动画。
///
/// # 示例
///
/// ```ignore
/// use gpui_component_motion::{MotionLifecycle, PresenceState, AnimationSpec};
/// use gpui::{div, px};
/// use std::time::Duration;
///
/// // 在 App::new 中创建（一次）
/// let weak = cx.entity().downgrade();
/// let presence = PresenceState::new(
///     cx,
///     "right-panel",
///     MotionLifecycle::slide_left(px(40.), AnimationSpec::default()),
///     move |window, cx| {
///         if let Some(strong) = weak.upgrade() {
///             strong.read(cx).render_panel_inner(window, cx)
///         } else {
///             div()
///         }
///     },
/// );
///
/// // 每帧更新 + 插入
/// self.presence.update(cx, |s, cx| s.set_present(self.open, window, cx));
/// row.child(self.presence.clone());
/// ```
pub struct PresenceState {
    /// 外部期望状态（`present` 传入值）。
    target: bool,
    /// 实际渲染状态（退场动画结束后才变 `false`）。
    visible: bool,
    /// 退场动画进行中。
    closing: bool,
    /// ID 计数器，每次入场递增，编入 `ElementId` 保证唯一。
    epoch: usize,
    /// child 构造闭包。
    builder: ChildBuilder,
    /// 入场 / 退场动画配对。
    lifecycle: MotionLifecycle,
    /// 进行中过渡的入场快照（转换发生时捕获，不受后续 `set_lifecycle` 影响）。
    enter_active: Option<(Motion, AnimationSpec)>,
    /// 进行中过渡的退场快照。
    exit_active: Option<(Motion, AnimationSpec)>,
    /// ID 基名（与 epoch 组合生成 `ElementId::NamedInteger`）。
    id: SharedString,
    /// 退场计时器，存储以便中断时取消。
    exit_task: Option<Task<()>>,
}

impl PresenceState {
    /// 创建 `PresenceState` 实体。
    ///
    /// `builder` 闭包在每帧被调用以构造 child 元素。
    /// 闭包应捕获 `WeakEntity` 以读取最新状态，避免循环引用。
    pub fn new<F>(
        cx: &mut App,
        id: impl Into<SharedString>,
        lifecycle: MotionLifecycle,
        builder: F,
    ) -> Entity<Self>
    where
        F: Fn(&mut Window, &mut App) -> Div + 'static,
    {
        cx.new(|_cx| Self {
            target: false,
            visible: false,
            closing: false,
            epoch: 0,
            builder: Rc::new(builder),
            lifecycle,
            enter_active: None,
            exit_active: None,
            id: id.into(),
            exit_task: None,
        })
    }

    /// 更新动画配对（运行时切换缓动模式等）。
    ///
    /// 进行中的过渡不受影响（读转换时捕获的快照），新 lifecycle 仅对后续过渡生效。
    /// 与当前 lifecycle 相等时直接返回（不通知、不触碰快照）。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use gpui_component_motion::{AnimationSpec, MotionLifecycle, SpringPreset};
    /// use gpui::px;
    ///
    /// // 从 Easing 切换到 Spring
    /// presence.update(cx, |s, cx| {
    ///     s.set_lifecycle(
    ///         MotionLifecycle::expand_width(
    ///             px(340.),
    ///             AnimationSpec::default().with_spring(SpringPreset::Default),
    ///         ),
    ///         cx,
    ///     );
    /// });
    /// ```
    pub fn set_lifecycle(&mut self, lifecycle: MotionLifecycle, cx: &mut Context<Self>) {
        if lifecycle == self.lifecycle {
            return;
        }
        self.lifecycle = lifecycle;
        cx.notify();
    }

    /// 更新期望状态。
    ///
    /// - `true`：元素应存在（新进入或中断退场时重入场，`epoch` 总是递增，
    ///   以保证入场动画 ID 与历史所有动画（含上次入场、上次退场）不同，
    ///   避免 GPUI `AnimationState` 缓存命中 `delta=1.0` 导致的"无动画瞬现"）。
    /// - `false`：元素应卸载（先播退场动画，结束后才真正移除）。
    /// - 退场中再次 `true`：中断退场，取消计时器，递增 epoch，重新播入场动画。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// // 打开面板
    /// presence.update(cx, |s, cx| s.set_present(true, window, cx));
    ///
    /// // 关闭面板（触发退场动画，动画结束后自动卸载）
    /// presence.update(cx, |s, cx| s.set_present(false, window, cx));
    ///
    /// // 退场中再次打开（中断退场，重新入场）
    /// presence.update(cx, |s, cx| s.set_present(true, window, cx));
    /// ```
    pub fn set_present(&mut self, present: bool, window: &mut Window, cx: &mut Context<Self>) {
        if present == self.target {
            return;
        }
        self.target = present;
        if present {
            // 进入入场态：不管是从 HIDDEN（visible=false）
            // 还是从中断 EXITING（closing=true, visible=true）进入，
            // 都必须递增 epoch——后者是最易被忽略的 bug 来源：
            // 若 epoch 不变，入场 ID（epoch*2 偶数）会与关闭前那次
            // 入场的 ID 完全相同 → GPUI 缓存的 AnimationState 已是
            // delta=1.0 的完成态 → 面板"哐当"一下以全宽出现无动画。
            self.epoch += 1;
            // 捕获本次入场的快照，进行中的过渡不受 set_lifecycle 影响（S6）。
            self.enter_active = Some((self.lifecycle.enter, self.lifecycle.enter_spec));
            self.exit_active = None;
            if self.closing {
                self.closing = false;
                // 丢弃任务即取消定时器（C8）。
                self.exit_task = None;
            }
            self.visible = true;
        } else if self.visible && !self.closing {
            // 启动退场：递增 epoch 保证退场 ID（epoch*2+1 奇数）
            // 不与任何历史退场/入场 ID 冲突。
            self.epoch += 1;
            // 捕获本次退场的快照（S6）。
            self.enter_active = None;
            self.exit_active = Some((self.lifecycle.exit, self.lifecycle.exit_spec));
            self.closing = true;
            // S7 宽限：定时器 = 快照退场时长 + delay + GRACE（兜底 C5 首帧滞后）。
            let exit_spec = self.lifecycle.exit_spec;
            let exit_duration = exit_spec.duration + exit_spec.delay + EXIT_TIMER_GRACE;
            // S5 epoch 守卫：spawn 前捕获，回调仅当 epoch 匹配且仍处 closing 时生效。
            let epoch = self.epoch;
            self.exit_task = Some(cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(exit_duration).await;
                _ = this.update_in(cx, |s, _window, cx| {
                    if s.closing && s.epoch == epoch {
                        s.closing = false;
                        s.visible = false;
                        s.enter_active = None;
                        s.exit_active = None;
                        s.exit_task = None;
                        cx.notify();
                    }
                });
            }));
        }
        cx.notify();
    }
}

impl Render for PresenceState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.visible {
            // 明确归零：高度保持 h_full 使父 flex row 不发生垂直跳动；
            // 宽度 0 + flex_shrink_0 确保在父容器中不占任何水平空间。
            return div().w(px(0.0)).h_full().flex_shrink_0().into_any_element();
        }

        let child = (self.builder)(window, cx);
        // overflow_hidden 容器 + flex_shrink_0：
        // - overflow_hidden 使内部 340px 固定宽度被外层 ExpandWidth 动画的 w(t) 正确裁剪
        // - flex_shrink_0 保证在父 h_flex 宽度不足时动画容器不会被压缩（否则
        //   测量出的宽度 < 340px 会导致"动画还没开始内容已经被挤扁再弹回"的视觉跳跃）
        let child = div()
            .h_full()
            .overflow_hidden()
            .flex_shrink_0()
            .child(child);
        let id_base = self.id.clone();
        let epoch = self.epoch;

        if self.closing {
            // 退场：用奇数 epoch 避免与入场动画 ID 冲突（GPUI 会跳过已完成动画）。
            // 优先读退场快照，`unwrap_or` 仅作防御性兜底。
            let id = ElementId::NamedInteger(id_base, (epoch * 2 + 1) as u64);
            let (motion, spec) = self
                .exit_active
                .unwrap_or((self.lifecycle.exit, self.lifecycle.exit_spec));
            Animated::new(child, id, spec, motion)
                .exit()
                .into_any_element()
        } else {
            // 入场：用偶数 epoch，每次重开都是新 ID。
            // 优先读入场快照，`unwrap_or` 仅作防御性兜底。
            let id = ElementId::NamedInteger(id_base, (epoch * 2) as u64);
            let (motion, spec) = self
                .enter_active
                .unwrap_or((self.lifecycle.enter, self.lifecycle.enter_spec));
            Animated::new(child, id, spec, motion).into_any_element()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, Empty, TestAppContext};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// 在窗口创建闭包内构造 `PresenceState`（窗口上下文创建 → 种子化
    /// `current_window`），使退场定时器回调中的 `update_in` 可解析窗口。
    fn setup(
        cx: &mut TestAppContext,
        lifecycle: MotionLifecycle,
    ) -> (gpui::WindowHandle<Empty>, Entity<PresenceState>) {
        let presence_cell: Rc<RefCell<Option<Entity<PresenceState>>>> = Default::default();
        let window = cx.add_window(|_window, cx| {
            *presence_cell.borrow_mut() = Some(PresenceState::new(
                cx,
                "panel",
                lifecycle,
                |_window, _cx| div().child("content"),
            ));
            Empty
        });
        let presence = presence_cell.borrow().clone().expect("presence created");
        (window, presence)
    }

    fn set_present(
        cx: &mut TestAppContext,
        presence: &Entity<PresenceState>,
        window: &gpui::WindowHandle<Empty>,
        present: bool,
    ) {
        cx.update_window(**window, |_, window, cx| {
            presence.update(cx, |s, cx| s.set_present(present, window, cx));
        })
        .unwrap();
    }

    fn state(cx: &TestAppContext, presence: &Entity<PresenceState>) -> (bool, bool, bool) {
        cx.update(|cx| {
            let s = presence.read(cx);
            (s.visible, s.closing, s.exit_task.is_some())
        })
    }

    /// T10：正常退场——推进 `exit total + GRACE` 后卸载。
    #[gpui::test]
    async fn normal_exit_unmounts_after_grace(cx: &mut TestAppContext) {
        let (window, presence) = setup(cx, MotionLifecycle::fade(AnimationSpec::default()));

        // 入场
        set_present(cx, &presence, &window, true);
        let (visible, closing, _) = state(cx, &presence);
        assert!(visible);
        assert!(!closing);

        // 启动退场
        set_present(cx, &presence, &window, false);
        let (closing, has_task, _) = state(cx, &presence);
        assert!(closing);
        assert!(has_task);
        let has_exit_snapshot = cx.update(|cx| presence.read(cx).exit_active.is_some());
        assert!(has_exit_snapshot);

        // 推进到定时器截止（200ms + 50ms grace）
        cx.executor().advance_clock(Duration::from_millis(250));
        cx.run_until_parked();

        let (visible, closing, has_task) = state(cx, &presence);
        assert!(!visible);
        assert!(!closing);
        assert!(!has_task);
        let (has_exit, has_enter) = cx.update(|cx| {
            (
                presence.read(cx).exit_active.is_some(),
                presence.read(cx).enter_active.is_some(),
            )
        });
        assert!(!has_exit);
        assert!(!has_enter);
    }

    /// T11：退场中途重开——旧定时器过期后必须被忽略（epoch 守卫）。
    #[gpui::test]
    async fn stale_exit_timer_ignored_after_reopen(cx: &mut TestAppContext) {
        let (window, presence) = setup(cx, MotionLifecycle::fade(AnimationSpec::default()));

        // true → false：启动退场，定时器 = 200ms + 50ms
        set_present(cx, &presence, &window, true);
        set_present(cx, &presence, &window, false);

        // 推进半程（100ms）
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();

        // 重开：取消定时器、递增 epoch、重新入场
        set_present(cx, &presence, &window, true);
        let (visible, closing, has_task) = state(cx, &presence);
        assert!(visible);
        assert!(!closing);
        assert!(!has_task);

        // 推进越过旧定时器截止（再 300ms > 250ms），过期事件必须无副作用
        cx.executor().advance_clock(Duration::from_millis(300));
        cx.run_until_parked();

        let (visible, closing, _) = state(cx, &presence);
        assert!(visible);
        assert!(!closing);
    }

    /// T12：退场中途 `set_lifecycle`——进行中的退场按转换时快照执行。
    #[gpui::test]
    async fn set_lifecycle_mid_exit_uses_snapshot(cx: &mut TestAppContext) {
        let (window, presence) = setup(
            cx,
            MotionLifecycle::fade(
                AnimationSpec::default().with_duration(Duration::from_millis(200)),
            ),
        );

        // 入场并完成
        set_present(cx, &presence, &window, true);
        cx.executor().advance_clock(Duration::from_millis(300));
        cx.run_until_parked();

        // 启动退场：定时器 = 200ms + 50ms grace
        set_present(cx, &presence, &window, false);

        // 推进半程后切换 lifecycle（退场 500ms）——不影响进行中的退场
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        cx.update(|cx| {
            presence.update(cx, |s, cx| {
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

        let (visible, closing, has_task) = state(cx, &presence);
        assert!(!visible, "应按原 200ms 快照卸载");
        assert!(!closing);
        assert!(!has_task);
    }
}
