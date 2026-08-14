//! 手势驱动弹簧（Phase 3 / D9）。
//!
//! [`DragSpring`] 是 [`crate::SpringValue`] 的手势语义封装：不暴露"目标值"声明，
//! 而是暴露拖拽生命周期（按住 / 拖动 / 松手），把手势坐标映射为弹簧目标：
//!
//! - **拖拽期**：目标 = 指针位置（[`DragSpring::drag_to`]），弹簧每帧追赶指针，
//!   产生轻微阻尼的跟手效果；
//! - **松手**：目标 = 调用方指定的 settle 点（[`DragSpring::end_drag`]），
//!   弹簧从当前值回弹并精确收敛到该点。
//!
//! 插值复用 [`crate::spring_value::spring_value_at`]，与 [`crate::SpringPreset`]
//! 曲线完全一致（含欠阻尼预设的过冲）。**无速度连续性**：重定向
//! （`drag_to` / `end_drag`）总是从当前值起跳，不做速度衔接，与 S8 打断跳变同类；
//! velocity-preserving 仿真留待 backlog。
//!
//! # 典型接入方式
//!
//! 拖拽控件（抽屉、卡片、滑块）的事件接线：
//!
//! ```ignore
//! // on_mouse_down：进入跟手模式，弹簧停在当前值
//! spring.update(cx, |s, cx| s.begin_drag(window, cx));
//! // on_mouse_move：目标 = 指针位置，弹簧阻尼追赶
//! spring.update(cx, |s, cx| s.drag_to(pointer_x, window, cx));
//! // on_mouse_up：目标 = settle 点，松手回弹
//! spring.update(cx, |s, cx| s.end_drag(settle_x, window, cx));
//! ```
//!
//! # 典型用途
//!
//! - **抽屉拖拽关闭**：拖拽期值跟随指针，松手 settle 到关闭位 / 阈值另一侧
//! - **卡片拖走**（swipe）：松手 settle 到屏幕外或回弹原位
//! - **滑块跟手**：thumb 位置 = [`DragSpring::value`]，拖动期阻尼追赶，松手 settle 到刻度

use std::time::Duration;

use gpui::{App, AppContext, Context, Entity, Task, Window};

use crate::SpringPreset;
use crate::spring_value::spring_value_at;

/// 每帧近似时长：16ms（约 60fps 一帧，与 [`crate::SpringValue`] 一致）。
const FRAME: Duration = Duration::from_millis(16);

/// 手势驱动弹簧数值实体。
///
/// 通过 [`DragSpring::new`] 创建 `Entity<DragSpring>`，事件驱动地调用
/// 拖拽生命周期方法（[`DragSpring::begin_drag`] -> [`DragSpring::drag_to`] ->
/// [`DragSpring::end_drag`]），内部以固定帧率（16ms）驱动弹簧插值，
/// 每帧 `notify` 订阅者；订阅者在 render 中读取 [`DragSpring::value`] 渲染。
///
/// # 示例
///
/// ```ignore
/// use gpui_component_motion::{DragSpring, SpringPreset};
///
/// // 创建（一次）：初始值 0
/// let spring = DragSpring::new(cx, 0.0, SpringPreset::Default);
///
/// // 拖拽事件接线（见模块文档）：按下 -> 拖动 -> 松手
/// spring.update(cx, |s, cx| s.begin_drag(window, cx));
/// spring.update(cx, |s, cx| s.drag_to(pointer_x, window, cx));
/// spring.update(cx, |s, cx| s.end_drag(settle_x, window, cx));
///
/// // render 中读取当前值
/// let current = spring.read(cx).value();
/// ```
pub struct DragSpring {
    /// 当前渲染值（弹簧插值结果；拖拽 / settle 进行中的瞬时值）。
    value: f32,
    /// 本次动画起点（`drag_to` / `end_drag` 时的当前值，重定向即重置）。
    from: f32,
    /// 目标值（拖拽期为指针位置，松手期为 settle 点）。
    target: f32,
    /// 弹簧预设（决定插值曲线与推荐时长）。
    preset: SpringPreset,
    /// 是否处于跟手模式（`begin_drag` 与 `end_drag` 之间）。
    dragging: bool,
    /// 世代守卫：每次重定向（`drag_to` / `end_drag` / `begin_drag`）递增；
    /// 过期 tick（旧拖拽 / 旧 settle）据此识别并忽略。
    generation: u64,
    /// 进行中的 tick 任务，存储以便中途取消（丢弃即取消，C8）。
    task: Option<Task<()>>,
}

impl DragSpring {
    /// 创建 `DragSpring` 实体：初始值 = 目标值 = `initial`，无进行中任务，非拖拽态。
    pub fn new(cx: &mut App, initial: f32, preset: SpringPreset) -> Entity<Self> {
        cx.new(|_cx| Self {
            value: initial,
            from: initial,
            target: initial,
            preset,
            dragging: false,
            generation: 0,
            task: None,
        })
    }

    /// 当前渲染值（弹簧插值进行中的瞬时值）。
    ///
    /// 渲染期每帧读取；未开始 / 已结束动画时等于目标值
    /// （拖拽中等于最近一次 [`drag_to`](Self::drag_to) 的指针位置）。
    pub fn value(&self) -> f32 {
        self.value
    }

    /// 是否处于跟手模式（`begin_drag` 后、`end_drag` 前）。
    ///
    /// 可用于区分"拖拽中"与"松手回弹"两个阶段以改变渲染样式
    /// （如拖拽中降低透明度、松手后恢复）。
    pub fn dragging(&self) -> bool {
        self.dragging
    }

    /// 进入跟手模式（典型接线：`on_mouse_down`）。
    ///
    /// - `dragging = true`；丢弃进行中的 settle 任务（C8，取消回弹）；
    /// - `from = target = 当前值`：弹簧停在当前位置，等待 `drag_to` 驱动；
    /// - 不 spawn 常驻任务——跟手期间由每次 [`drag_to`](Self::drag_to) 驱动。
    pub fn begin_drag(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.dragging = true;
        // 丢弃旧任务即取消旧 tick（C8）；世代自增兜底"已派发但未执行"的过期回调。
        self.task = None;
        self.generation += 1;
        // 弹簧停在当前值：进入跟手模式后值由 drag_to 驱动。
        self.from = self.value;
        self.target = self.value;
        cx.notify();
    }

    /// 拖拽中更新指针目标（典型接线：`on_mouse_move`）。
    ///
    /// 仅当处于跟手模式（`dragging`）时有效；非拖拽态调用被忽略（幂等，
    /// 不改变任何状态、不启动任务）。生效时：
    ///
    /// - `from = 当前值`：从当前位置起跳（无跳变）；
    /// - `target = value`：弹簧以推荐时长追赶指针，产生轻微阻尼跟手感；
    /// - 世代自增并重启 tick，旧拖拽 tick 被丢弃（取消）或被世代守卫忽略。
    pub fn drag_to(&mut self, value: f32, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dragging {
            return;
        }
        self.from = self.value;
        self.target = value;
        self.spawn_tick(window, cx);
        cx.notify();
    }

    /// 松手回弹（典型接线：`on_mouse_up`）。
    ///
    /// - 退出跟手模式（`dragging = false`）；
    /// - `from = 当前值`、`target = settle`：弹簧从松手位置回弹到 settle 点，
    ///   动画结束后精确收敛（`t >= 1` 无残差）且任务结束；
    /// - 世代自增并重启 tick，旧拖拽 tick 被丢弃（取消）或被世代守卫忽略。
    pub fn end_drag(&mut self, settle: f32, window: &mut Window, cx: &mut Context<Self>) {
        self.dragging = false;
        self.from = self.value;
        self.target = settle;
        self.spawn_tick(window, cx);
        cx.notify();
    }

    /// 启动弹簧追赶 tick：世代自增，捕获本次动画的快照（generation / preset /
    /// 总时长），以 16ms 帧率插值到目标值，结束或过期后清理。
    fn spawn_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        let preset = self.preset;
        let total = preset.recommended_duration();
        let mut elapsed = Duration::ZERO;
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(FRAME).await;
                elapsed += FRAME;
                let t = elapsed.as_secs_f32() / total.as_secs_f32();
                let done = t >= 1.0;
                _ = this.update_in(cx, |s, _window, cx| {
                    // 世代守卫：过期 tick（旧拖拽 / 旧 settle）不得污染新动画。
                    if s.generation != generation {
                        return;
                    }
                    if done {
                        s.value = s.target;
                        s.task = None;
                    } else {
                        s.value = spring_value_at(s.from, s.target, s.preset, t);
                    }
                    cx.notify();
                });
                if done {
                    break;
                }
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Empty, TestAppContext};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// 在窗口创建闭包内构造 `DragSpring`（窗口上下文创建 -> 种子化
    /// `current_window`），使 tick 回调中的 `update_in` 可解析窗口。
    fn setup(
        cx: &mut TestAppContext,
        preset: SpringPreset,
    ) -> (gpui::WindowHandle<Empty>, Entity<DragSpring>) {
        let cell: Rc<RefCell<Option<Entity<DragSpring>>>> = Default::default();
        let window = cx.add_window(|_window, cx| {
            *cell.borrow_mut() = Some(DragSpring::new(cx, 0.0, preset));
            Empty
        });
        let ds = cell.borrow().clone().expect("drag spring created");
        (window, ds)
    }

    fn begin(cx: &mut TestAppContext, ds: &Entity<DragSpring>, window: &gpui::WindowHandle<Empty>) {
        cx.update_window(**window, |_, window, cx| {
            ds.update(cx, |s, cx| s.begin_drag(window, cx));
        })
        .unwrap();
    }

    fn drag_to(
        cx: &mut TestAppContext,
        ds: &Entity<DragSpring>,
        window: &gpui::WindowHandle<Empty>,
        v: f32,
    ) {
        cx.update_window(**window, |_, window, cx| {
            ds.update(cx, |s, cx| s.drag_to(v, window, cx));
        })
        .unwrap();
    }

    fn end_drag(
        cx: &mut TestAppContext,
        ds: &Entity<DragSpring>,
        window: &gpui::WindowHandle<Empty>,
        settle: f32,
    ) {
        cx.update_window(**window, |_, window, cx| {
            ds.update(cx, |s, cx| s.end_drag(settle, window, cx));
        })
        .unwrap();
    }

    fn value(cx: &TestAppContext, ds: &Entity<DragSpring>) -> f32 {
        cx.update(|cx| ds.read(cx).value())
    }

    fn dragging(cx: &TestAppContext, ds: &Entity<DragSpring>) -> bool {
        cx.update(|cx| ds.read(cx).dragging())
    }

    fn has_task(cx: &TestAppContext, ds: &Entity<DragSpring>) -> bool {
        cx.update(|cx| ds.read(cx).task.is_some())
    }

    /// 重定向：拖拽中 `drag_to` 从当前值起跳——半程记录 `mid` 后重定向回 0，
    /// 逐帧断言不向上跳变（首帧不越过 `mid`），最终精确收敛到新目标。
    #[gpui::test]
    async fn drag_to_retargets_from_current(cx: &mut TestAppContext) {
        let (window, ds) = setup(cx, SpringPreset::Default);
        begin(cx, &ds, &window);
        assert!(dragging(cx, &ds));
        drag_to(cx, &ds, &window, 100.0);
        let total = SpringPreset::Default.recommended_duration();

        // 半程：值应处于 (0, 100) 之间。
        cx.executor().advance_clock(total / 2);
        cx.run_until_parked();
        let mid = value(cx, &ds);
        assert!(mid > 0.0 && mid < 100.0, "半程应处于 0..100 之间: {mid}");

        // 重定向回 0：from = 当前值，从当前值起跳。
        drag_to(cx, &ds, &window, 0.0);

        // 逐帧推进：值不得向上跳变（超过重定向点 mid），最终收敛到 0。
        for _ in 0..80 {
            cx.executor().advance_clock(Duration::from_millis(16));
            cx.run_until_parked();
            let v = value(cx, &ds);
            assert!(v <= mid + 1e-3, "重定向后不得向上跳变: v={v} > mid={mid}");
            if v == 0.0 {
                break;
            }
        }
        cx.executor().advance_clock(total * 2);
        cx.run_until_parked();
        assert_eq!(value(cx, &ds), 0.0, "最终应精确收敛到新目标");
        assert!(!has_task(cx, &ds), "动画结束后任务应结束");
    }

    /// 松手 settle：end_drag 后弹簧从当前值回弹，推进推荐时长后精确收敛
    /// 到 settle 点、任务结束、退出跟手模式。
    #[gpui::test]
    async fn end_drag_settles_to_target(cx: &mut TestAppContext) {
        let (window, ds) = setup(cx, SpringPreset::Default);
        begin(cx, &ds, &window);
        drag_to(cx, &ds, &window, 100.0);
        end_drag(cx, &ds, &window, 0.0);
        assert!(!dragging(cx, &ds), "end_drag 后应退出跟手模式");
        assert!(has_task(cx, &ds), "end_drag 应启动 settle 任务");

        let total = SpringPreset::Default.recommended_duration();
        cx.executor()
            .advance_clock(total + Duration::from_millis(100));
        cx.run_until_parked();

        assert_eq!(value(cx, &ds), 0.0, "松手后应精确收敛到 settle 点");
        assert!(!has_task(cx, &ds), "settle 结束后任务应结束");
        assert!(!dragging(cx, &ds));
    }

    /// 重按打断 settle：settle 进行中 begin_drag 丢弃任务并停在当前值，
    /// 推进时钟越过 settle 原定截止无副作用（任务已取消，值不再变化）。
    #[gpui::test]
    async fn begin_drag_stops_settle(cx: &mut TestAppContext) {
        let (window, ds) = setup(cx, SpringPreset::Default);
        begin(cx, &ds, &window);
        drag_to(cx, &ds, &window, 100.0);
        end_drag(cx, &ds, &window, 50.0);

        // 推进 settle 半程：值已从 0 向 50 移动。
        let total = SpringPreset::Default.recommended_duration();
        cx.executor().advance_clock(total / 2);
        cx.run_until_parked();
        let mid = value(cx, &ds);
        assert!(mid > 0.0 && mid < 50.0, "半程应处于 0..50 之间: {mid}");

        // 立即重新按住：丢弃 settle 任务，进入跟手模式，弹簧停在当前值。
        begin(cx, &ds, &window);
        assert!(dragging(cx, &ds));
        assert!(!has_task(cx, &ds), "begin_drag 应丢弃 settle 任务");

        // 推进时钟越过 settle 原定截止：旧任务已取消（世代守卫），无副作用。
        cx.executor().advance_clock(total * 2);
        cx.run_until_parked();
        assert_eq!(value(cx, &ds), mid, "旧 settle 不得再推进值");
        assert!(!has_task(cx, &ds), "旧任务不得复活");
        assert!(dragging(cx, &ds), "仍处于跟手模式");
    }

    /// 非拖拽态 `drag_to` 被忽略：未 begin_drag 时调用不改变值、不启动任务，
    /// 推进时钟也不应有任何动画发生。
    #[gpui::test]
    async fn non_dragging_drag_to_ignored(cx: &mut TestAppContext) {
        let (window, ds) = setup(cx, SpringPreset::Default);
        drag_to(cx, &ds, &window, 100.0);
        assert_eq!(value(cx, &ds), 0.0, "非拖拽态 drag_to 不得改变值");
        assert!(!has_task(cx, &ds), "非拖拽态 drag_to 不得启动任务");
        assert!(!dragging(cx, &ds));

        let total = SpringPreset::Default.recommended_duration();
        cx.executor().advance_clock(total * 2);
        cx.run_until_parked();
        assert_eq!(value(cx, &ds), 0.0);
        assert!(!has_task(cx, &ds));
    }

    /// 过冲（端到端）：Wobbly 松手 settle 过程中瞬时值越过 settle 点
    /// （欠阻尼过冲），随后精确收敛到 settle 点并结束任务。
    #[gpui::test]
    async fn wobbly_settle_overshoots_then_converges(cx: &mut TestAppContext) {
        let (window, ds) = setup(cx, SpringPreset::Wobbly);
        begin(cx, &ds, &window);
        drag_to(cx, &ds, &window, 100.0);
        end_drag(cx, &ds, &window, 100.0);
        let total = SpringPreset::Wobbly.recommended_duration();

        // 逐帧采样全程：应观测到瞬时值超过 100（过冲）。
        let steps = (total.as_millis() as usize / 16) + 2;
        let mut max_seen = 0.0_f32;
        for _ in 0..steps {
            cx.executor().advance_clock(Duration::from_millis(16));
            cx.run_until_parked();
            max_seen = max_seen.max(value(cx, &ds));
        }
        assert!(max_seen > 100.0, "Wobbly settle 应出现过冲: max={max_seen}");

        cx.executor().advance_clock(total * 2);
        cx.run_until_parked();
        assert_eq!(value(cx, &ds), 100.0, "过冲后最终必须精确收敛到 settle 点");
        assert!(!has_task(cx, &ds), "动画结束后任务应结束");
    }
}
