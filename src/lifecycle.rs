//! 入场 + 退场动画配对。
//!
//! [`MotionLifecycle`] 描述一个元素在 mount 与 unmount 时分别使用的动效，
//! 供 [`crate::PresenceState`] 使用——调用方只声明"元素何时该存在"，
//! 框架自动按配对播入场 / 退场动画。

use gpui::Pixels;

use crate::{AnimationSpec, Motion};

/// 描述一个元素的入场 + 退场动画配对。
///
/// `enter` 与 `exit` 可以是不同的 [`Motion`]（例如入场滑入、退场淡出），
/// 也可以使用不同的 [`AnimationSpec`]（例如退场更快）。
///
/// 退场规格经 [`AnimationSpec::without_spring`] 强制剥离 Spring（I3：退场曲线严格单调）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionLifecycle {
    /// 入场动效。
    pub enter: Motion,
    /// 退场动效。
    pub exit: Motion,
    /// 入场规格（时长 / 延迟 / 缓动 / 可选 Spring 预设）。
    pub enter_spec: AnimationSpec,
    /// 退场规格（时长 / 延迟 / 缓动）。
    pub exit_spec: AnimationSpec,
}

impl MotionLifecycle {
    /// 创建配对，入场与退场共用同一规格。
    ///
    /// 退场规格经 `without_spring()` 剥离 Spring（S3 规则：退场剥离 Spring、未显式覆盖时重置 200ms）。
    pub fn new(enter: Motion, exit: Motion, spec: AnimationSpec) -> Self {
        Self {
            enter,
            exit,
            enter_spec: spec,
            exit_spec: spec.without_spring(),
        }
    }

    /// 仅覆盖退场规格，入场规格保持不变。
    ///
    /// 传入的规格经 `without_spring()` 剥离 Spring（S3 规则：退场剥离 Spring、未显式覆盖时重置 200ms）。
    pub fn with_exit_spec(mut self, spec: AnimationSpec) -> Self {
        self.exit_spec = spec.without_spring();
        self
    }

    /// 淡入淡出：入场与退场均为 [`Motion::Fade`]。
    pub fn fade(spec: AnimationSpec) -> Self {
        Self::new(Motion::Fade, Motion::Fade, spec)
    }

    /// 从下方滑入 / 滑出。
    pub fn slide_up(offset: Pixels, spec: AnimationSpec) -> Self {
        Self::new(Motion::SlideUp(offset), Motion::SlideUp(offset), spec)
    }

    /// 从上方滑入 / 滑出。
    pub fn slide_down(offset: Pixels, spec: AnimationSpec) -> Self {
        Self::new(Motion::SlideDown(offset), Motion::SlideDown(offset), spec)
    }

    /// 从右侧滑入 / 滑出（用于右侧面板）。
    pub fn slide_left(offset: Pixels, spec: AnimationSpec) -> Self {
        Self::new(Motion::SlideLeft(offset), Motion::SlideLeft(offset), spec)
    }

    /// 从左侧滑入 / 滑出（用于左侧面板）。
    pub fn slide_right(offset: Pixels, spec: AnimationSpec) -> Self {
        Self::new(Motion::SlideRight(offset), Motion::SlideRight(offset), spec)
    }

    /// 宽度展开 / 收缩：入场时宽度从 0 到 max_width，退场反向。
    /// 适用于侧边栏 / 面板的展开收起，布局渐变而非瞬变。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use gpui_component_motion::{AnimationSpec, MotionLifecycle, SpringPreset};
    /// use gpui::px;
    ///
    /// let lc = MotionLifecycle::expand_width(
    ///     px(340.),
    ///     AnimationSpec::default().with_spring(SpringPreset::Default),
    /// )
    /// .with_exit_spec(AnimationSpec::default()); // 退场用传统缓动
    /// ```
    pub fn expand_width(max_width: Pixels, spec: AnimationSpec) -> Self {
        Self::new(
            Motion::ExpandWidth(max_width),
            Motion::ExpandWidth(max_width),
            spec,
        )
    }

    /// 高度展开 / 收缩：入场时高度从 0 到 max_height，退场反向。
    /// 适用于垂直方向的折叠面板（dropdown / accordion / collapse）。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use gpui_component_motion::{AnimationSpec, MotionLifecycle};
    /// use gpui::px;
    ///
    /// let lc = MotionLifecycle::expand_height(
    ///     px(200.),
    ///     AnimationSpec::default(),
    /// );
    /// ```
    pub fn expand_height(max_height: Pixels, spec: AnimationSpec) -> Self {
        Self::new(
            Motion::ExpandHeight(max_height),
            Motion::ExpandHeight(max_height),
            spec,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;
    use std::time::Duration;

    #[test]
    fn new_uses_enter_for_both_specs() {
        let spec = AnimationSpec::default();
        let lc = MotionLifecycle::new(Motion::Fade, Motion::SlideUp(px(10.0)), spec);
        assert_eq!(lc.enter_spec, spec);
        assert_eq!(lc.exit_spec, spec);
    }

    #[test]
    fn with_exit_spec_overrides_exit_only() {
        let enter_spec = AnimationSpec::default();
        let exit_spec = AnimationSpec::fast();
        let lc =
            MotionLifecycle::new(Motion::Fade, Motion::Fade, enter_spec).with_exit_spec(exit_spec);
        assert_eq!(lc.enter_spec, enter_spec);
        assert_eq!(lc.exit_spec, exit_spec);
    }

    #[test]
    fn fade_preset_uses_fade_for_both() {
        let lc = MotionLifecycle::fade(AnimationSpec::default());
        assert_eq!(lc.enter, Motion::Fade);
        assert_eq!(lc.exit, Motion::Fade);
    }

    #[test]
    fn slide_left_preset_uses_matching_motions() {
        let lc = MotionLifecycle::slide_left(px(40.0), AnimationSpec::default());
        assert_eq!(lc.enter, Motion::SlideLeft(px(40.0)));
        assert_eq!(lc.exit, Motion::SlideLeft(px(40.0)));
    }

    #[test]
    fn slide_presets_cover_all_directions() {
        let spec = AnimationSpec::default();
        let _ = MotionLifecycle::slide_up(px(10.0), spec);
        let _ = MotionLifecycle::slide_down(px(10.0), spec);
        let _ = MotionLifecycle::slide_left(px(10.0), spec);
        let _ = MotionLifecycle::slide_right(px(10.0), spec);
    }

    /// T9：`new` 从退场规格剥离 Spring（入场保留），时长按 S3 规则重置。
    #[test]
    fn new_strips_spring_from_exit() {
        let spring_spec = AnimationSpec::default().with_spring(crate::SpringPreset::Wobbly);
        let lc = MotionLifecycle::new(Motion::Fade, Motion::Fade, spring_spec);
        assert_eq!(lc.enter_spec.spring, Some(crate::SpringPreset::Wobbly));
        assert_eq!(lc.exit_spec.spring, None);
        assert_eq!(lc.exit_spec.duration, AnimationSpec::default().duration);
        assert_eq!(lc.exit_spec.easing, spring_spec.easing);
        assert_eq!(lc.exit_spec.delay, spring_spec.delay);
    }

    /// T9 补充：`with_exit_spec` 传入 Spring 规格同样剥离，显式时长保留。
    #[test]
    fn with_exit_spec_strips_spring_keeps_explicit_duration() {
        let lc = MotionLifecycle::fade(AnimationSpec::default()).with_exit_spec(
            AnimationSpec::default()
                .with_spring(crate::SpringPreset::Default)
                .with_duration(Duration::from_millis(600)),
        );
        assert_eq!(lc.exit_spec.spring, None);
        assert_eq!(lc.exit_spec.duration, Duration::from_millis(600));
    }

    /// `set_lifecycle` 相等短路依赖 `PartialEq`（S6）。
    #[test]
    fn lifecycle_partial_eq() {
        let a = MotionLifecycle::fade(AnimationSpec::default());
        let b = MotionLifecycle::fade(AnimationSpec::default());
        assert_eq!(a, b);
        let c = b.with_exit_spec(AnimationSpec::fast());
        assert_ne!(a, c);
    }
}
