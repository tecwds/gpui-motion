//! 声明式数值弹簧（Phase 2 / D8）。
//!
//! [`SpringValue`] 是一个基于 [`SpringPreset`] 曲线的声明式数值动画：
//! 调用方只声明"数值应当变成多少"（[`SpringValue::set_target`]），
//! 内部以固定帧率（16ms ≈ 60fps）驱动插值，每帧 `notify` 订阅者，
//! 由订阅者在 render 中读取 [`SpringValue::value`] 渲染。
//!
//! 与 [`crate::PresenceState`] 同属声明式动画家族：不暴露动画时序，
//! 只暴露目标状态，中间过程（弹簧插值）由框架完成。
//!
//! # 典型用途
//!
//! - 数字滚动 / 计数器（数字从 0 弹到 100）
//! - 进度条、加载指示器
//! - 开关滑块、可拖拽控件的回弹
//! - 图表数据点过渡
//!
//! # 语义
//!
//! - 插值系数与 [`SpringPreset`] 的曲线完全一致（`from + (to - from) * progress(t)`），
//!   `t ≥ 1` 时精确收敛到目标值（无残差）。
//! - 欠阻尼预设（`Default` / `Gentle` / `Wobbly`）中间允许过冲，
//!   即瞬时值可能越出 `[min(from, to), max(from, to)]` 区间。
//! - **重定向无速度连续性**：动画中途再次 [`set_target`](SpringValue::set_target)
//!   时从"当前值"起跳（`from = current`），不做速度衔接，
//!   与 S8 打断跳变同类；velocity-preserving 仿真留待 backlog。

use std::time::Duration;

use gpui::{App, AppContext, Context, Entity, Task, Window};

use crate::SpringPreset;
use crate::easing::spring_progress;

/// 每帧近似时长：16ms ≈ 60fps 一帧。
const FRAME: Duration = Duration::from_millis(16);

/// 声明式数值弹簧。
///
/// 通过 [`SpringValue::new`] 创建 `Entity<SpringValue>`，事件驱动地调用
/// [`SpringValue::set_target`] 声明目标值，弹簧自动从当前值插值到目标值，
/// 每帧 `notify` 订阅者；订阅者在 render 中读取 [`SpringValue::value`]。
///
/// # 示例
///
/// ```ignore
/// use gpui_component_motion::{SpringValue, SpringPreset};
///
/// // 创建（一次）：初始值 0
/// let value = SpringValue::new(cx, 0.0, SpringPreset::Default);
///
/// // 事件驱动：用户滚动 / 切换时声明新目标
/// value.update(cx, |s, cx| s.set_target(100.0, window, cx));
///
/// // render 中读取当前值
/// let current = value.read(cx).value();
/// ```
pub struct SpringValue {
    /// 当前渲染值（弹簧插值结果）。
    current: f32,
    /// 目标值（本次动画终点）。
    target: f32,
    /// 本次动画起点（`set_target` 时的当前值，中途重定向即重置）。
    from: f32,
    /// 弹簧预设（决定插值曲线与推荐时长）。
    preset: SpringPreset,
    /// 世代守卫：每次 `set_target` 递增；过期 tick（旧动画）据此识别并忽略。
    generation: u64,
    /// 进行中的 tick 任务，存储以便中途取消（丢弃即取消，C8）。
    task: Option<Task<()>>,
}

impl SpringValue {
    /// 创建 `SpringValue` 实体，初始值 = 目标值 = `initial`，无进行中动画。
    pub fn new(cx: &mut App, initial: f32, preset: SpringPreset) -> Entity<Self> {
        cx.new(|_cx| Self {
            current: initial,
            target: initial,
            from: initial,
            preset,
            generation: 0,
            task: None,
        })
    }

    /// 当前渲染值（弹簧插值进行中的瞬时值）。
    ///
    /// 渲染期每帧读取；未开始 / 已结束动画时等于目标值。
    pub fn value(&self) -> f32 {
        self.current
    }

    /// 目标值（最近一次 [`set_target`](Self::set_target) 声明的终点）。
    pub fn target(&self) -> f32 {
        self.target
    }

    /// 声明目标值：弹簧从当前值插值到 `target`。
    ///
    /// - **短路**：无进行中动画且目标未变时直接返回（不 `notify`），
    ///   与 `PresenceState::set_present` 的短路语义一致。
    /// - **重定向**：动画中途再次调用时，`from` 重置为当前值（从当前值起跳），
    ///   旧 tick 任务被丢弃（取消）；无速度连续性（见模块文档）。
    /// - 每次调用（未短路时）都会 `notify` 一次，订阅者可立即读到新目标。
    pub fn set_target(&mut self, target: f32, window: &mut Window, cx: &mut Context<Self>) {
        if self.task.is_none() && target == self.target {
            return;
        }
        self.from = self.current;
        self.target = target;
        // 丢弃旧任务即取消旧 tick（C8）。
        self.task = None;
        // 世代守卫：新动画自增世代，旧 tick 回调即使复活也因世代不匹配被忽略。
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
                    // 世代守卫：旧动画的过期 tick 不得污染新动画。
                    if s.generation != generation {
                        return;
                    }
                    if done {
                        s.current = s.target;
                        s.task = None;
                    } else {
                        s.current = spring_value_at(s.from, s.target, s.preset, t);
                    }
                    cx.notify();
                });
                if done {
                    break;
                }
            }
        }));
        cx.notify();
    }
}

/// 弹簧插值纯函数：`from` 与 `to` 之间按 [`spring_progress`] 曲线插值。
///
/// `t ≥ 1` 精确返回 `to`；`0 ≤ t < 1` 保留过冲（欠阻尼预设瞬时值可越出区间）。
pub(crate) fn spring_value_at(from: f32, to: f32, preset: SpringPreset, t: f32) -> f32 {
    from + (to - from) * spring_progress(preset, t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, Empty, TestAppContext};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// 在窗口创建闭包内构造 `SpringValue`（窗口上下文创建 → 种子化
    /// `current_window`），使 tick 回调中的 `update_in` 可解析窗口。
    fn setup(
        cx: &mut TestAppContext,
        preset: SpringPreset,
    ) -> (gpui::WindowHandle<Empty>, Entity<SpringValue>) {
        let cell: Rc<RefCell<Option<Entity<SpringValue>>>> = Default::default();
        let window = cx.add_window(|_window, cx| {
            *cell.borrow_mut() = Some(SpringValue::new(cx, 0.0, preset));
            Empty
        });
        let sv = cell.borrow().clone().expect("spring created");
        (window, sv)
    }

    fn set_target(
        cx: &mut TestAppContext,
        sv: &Entity<SpringValue>,
        window: &gpui::WindowHandle<Empty>,
        target: f32,
    ) {
        cx.update_window(**window, |_, window, cx| {
            sv.update(cx, |s, cx| s.set_target(target, window, cx));
        })
        .unwrap();
    }

    fn value(cx: &TestAppContext, sv: &Entity<SpringValue>) -> f32 {
        cx.update(|cx| sv.read(cx).value())
    }

    fn has_task(cx: &TestAppContext, sv: &Entity<SpringValue>) -> bool {
        cx.update(|cx| sv.read(cx).task.is_some())
    }

    /// 纯函数：t=0 → from；t≥1 → to（精确，无残差）；逐点与 `spring_progress` 一致。
    #[test]
    fn spring_value_at_matches_spring_progress() {
        let (from, to) = (10.0_f32, 250.0_f32);
        for preset in [
            SpringPreset::Stiff,
            SpringPreset::Default,
            SpringPreset::Gentle,
            SpringPreset::Wobbly,
        ] {
            assert_eq!(
                spring_value_at(from, to, preset, 0.0),
                from,
                "t=0 应等于 from"
            );
            assert_eq!(
                spring_value_at(from, to, preset, 1.0),
                to,
                "t=1 应精确等于 to"
            );
            assert_eq!(
                spring_value_at(from, to, preset, 1.5),
                to,
                "t>1 应精确等于 to"
            );
            for i in 0..=100 {
                let t = i as f32 / 100.0;
                let expected = from + (to - from) * spring_progress(preset, t);
                assert_eq!(
                    spring_value_at(from, to, preset, t),
                    expected,
                    "preset={preset:?} t={t} 应与 spring_progress 逐点一致"
                );
            }
        }
    }

    /// 集成：`set_target(0→100)` 后推进 ≥ 推荐时长，最终精确收敛并结束任务。
    #[gpui::test]
    async fn reaches_target_after_recommended_duration(cx: &mut TestAppContext) {
        let (window, sv) = setup(cx, SpringPreset::Default);
        set_target(cx, &sv, &window, 100.0);

        // 未推进时钟：current 仍为旧值，tick 任务已启动。
        assert_eq!(value(cx, &sv), 0.0);
        assert!(has_task(cx, &sv));

        let total = SpringPreset::Default.recommended_duration();
        cx.executor()
            .advance_clock(total + Duration::from_millis(100));
        cx.run_until_parked();

        assert_eq!(value(cx, &sv), 100.0, "动画结束后应精确收敛到目标值");
        assert!(!has_task(cx, &sv), "动画结束后任务应结束");
    }

    /// 短路：目标未变且无进行中动画时不 spawn 新任务（含动画完成后的重复声明）。
    #[gpui::test]
    async fn same_target_short_circuits(cx: &mut TestAppContext) {
        let (window, sv) = setup(cx, SpringPreset::Default);

        // 初始 current == target == 0，无任务：设置相同目标应短路。
        set_target(cx, &sv, &window, 0.0);
        assert!(!has_task(cx, &sv), "相同目标不得 spawn 新任务");

        // 完成一次动画后再设相同目标：同样短路（不重启）。
        set_target(cx, &sv, &window, 50.0);
        cx.executor().advance_clock(
            SpringPreset::Default.recommended_duration() + Duration::from_millis(100),
        );
        cx.run_until_parked();
        assert_eq!(value(cx, &sv), 50.0);

        set_target(cx, &sv, &window, 50.0);
        assert!(!has_task(cx, &sv), "动画完成后设置相同目标不得重启动画");
    }

    /// 重定向：中途 `set_target` 从当前值起跳，旧 tick 到期事件不污染新动画，
    /// 且重定向后瞬时值不得向上跳变（单调逼近新目标）。
    #[gpui::test]
    async fn redirect_mid_animation(cx: &mut TestAppContext) {
        let (window, sv) = setup(cx, SpringPreset::Default);
        set_target(cx, &sv, &window, 100.0);
        let total = SpringPreset::Default.recommended_duration();

        // 半程：值应处于 (0, 100) 之间。
        cx.executor().advance_clock(total / 2);
        cx.run_until_parked();
        let mid = value(cx, &sv);
        assert!(mid > 0.0 && mid < 100.0, "半程应处于 0..100 之间: {mid}");

        // 重定向回 0：from = 当前值，从当前值起跳。
        set_target(cx, &sv, &window, 0.0);

        // 逐帧推进：值不得向上跳变（超过重定向点 mid），最终收敛到 0。
        for _ in 0..80 {
            cx.executor().advance_clock(Duration::from_millis(16));
            cx.run_until_parked();
            let v = value(cx, &sv);
            assert!(v <= mid + 1e-3, "重定向后不得向上跳变: v={v} > mid={mid}");
            if v == 0.0 {
                break;
            }
        }
        cx.executor().advance_clock(total * 2);
        cx.run_until_parked();
        assert_eq!(value(cx, &sv), 0.0, "最终应精确收敛到新目标");
        assert!(!has_task(cx, &sv), "动画结束后任务应结束");
    }

    /// 过冲（代替单调性断言）：欠阻尼预设（Wobbly）动画中瞬时值可越出
    /// `[min(from, to), max(from, to)]`，但最终必须精确收敛到目标。
    /// 过冲行为本身在 [`spring_value_at`] 单测中逐点覆盖。
    #[gpui::test]
    async fn wobbly_overshoots_then_converges(cx: &mut TestAppContext) {
        let (window, sv) = setup(cx, SpringPreset::Wobbly);
        set_target(cx, &sv, &window, 100.0);
        let total = SpringPreset::Wobbly.recommended_duration();

        // 逐帧采样全程：应观测到瞬时值超过 100（过冲）。
        let steps = (total.as_millis() as usize / 16) + 2;
        let mut max_seen = 0.0_f32;
        for _ in 0..steps {
            cx.executor().advance_clock(Duration::from_millis(16));
            cx.run_until_parked();
            max_seen = max_seen.max(value(cx, &sv));
        }
        assert!(max_seen > 100.0, "Wobbly 应出现过冲: max={max_seen}");

        cx.executor().advance_clock(total * 2);
        cx.run_until_parked();
        assert_eq!(value(cx, &sv), 100.0, "过冲后最终必须精确收敛到目标");
        assert!(!has_task(cx, &sv), "动画结束后任务应结束");
    }
}
