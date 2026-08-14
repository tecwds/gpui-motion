//! 预设动画效果。
//!
//! 每个 [`Motion`] 变体描述一种入场视觉效果，
//! 通过 [`Motion::apply`] 将缓动进度 `t`（`0 → 1`）映射到具体样式。
//!
//! 仅使用 `Styled` 提供的 `opacity` / `top` / `left` / `width` / `height`
//! 实现位移、淡入与尺寸展开——
//! GPUI 的 `Styled` 不支持 `scale` / `transform`，因此 `ScaleIn` / `Pop`
//! 等缩放类效果无法在不自定义 `Element` 的前提下实现，留待后续阶段。

use gpui::{Pixels, Styled, px};

/// 对 `Pixels` 做线性插值：`from + (to - from) * t`。
fn lerp_px(from: Pixels, to: Pixels, t: f32) -> Pixels {
    let a: f32 = from.into();
    let b: f32 = to.into();
    px(a + (b - a) * t)
}

/// 钳制到 `[0, 1]`。NaN 输入按 IEEE 754 语义透传（`f32::clamp` 不处理 NaN），
/// 由 [`crate::AnimationSpec`] 的零时长防护在源头消除，此处不额外处理。
pub(crate) fn clamp_unit(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}

/// 拒绝负值，允许上界过冲。
pub(crate) fn max0(t: f32) -> f32 {
    t.max(0.0)
}

/// 预设入场动画。
///
/// 变体携带的 `Pixels` 参数表示位移幅度（Slide*）或目标尺寸（Expand*）。
///
/// 所有变体均实现 [`Copy`]，可在动画规格间自由传递无需克隆。
///
/// # 示例
///
/// ```ignore
/// use gpui_component_motion::{Motion, AnimationSpec, Animated};
/// use gpui::{div, px, Styled};
///
/// // 通过 MotionExt 快捷方法（推荐）
/// use gpui_component_motion::MotionExt;
/// let el = div().fade_in("my-fade");
///
/// // 或通过 with_motion 手动构造
/// let el = div().with_motion(
///     "my-expand",
///     AnimationSpec::default(),
///     Motion::ExpandWidth(px(340.)),
/// );
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion {
    /// 透明度 `0 → 1`。
    Fade,
    /// 从下方滑入：`top` 从 `+offset` 到 `0`。
    SlideUp(Pixels),
    /// 从上方滑入：`top` 从 `-offset` 到 `0`。
    SlideDown(Pixels),
    /// 从右侧滑入：`left` 从 `+offset` 到 `0`。
    SlideLeft(Pixels),
    /// 从左侧滑入：`left` 从 `-offset` 到 `0`。
    SlideRight(Pixels),
    /// 宽度展开：`width` 从 `0` 到 `max_width`（入场）/ 反向（退场）。
    ExpandWidth(Pixels),
    /// 高度展开：`height` 从 `0` 到 `max_height`（入场）/ 反向（退场）。
    /// 适用于垂直方向的折叠面板（dropdown / accordion / collapse）。
    ExpandHeight(Pixels),
}

impl Motion {
    /// 将缓动进度 `t`（`0 → 1`，`0` 为起始状态、`1` 为结束状态）
    /// 应用到元素 `el` 上，返回应用了样式的元素。
    ///
    /// `t = 0` 时元素处于入场前状态（透明 / 偏移 / 零宽高）。
    /// `t = 1` 时应用 Motion 的**终态样式**，覆盖元素上与该动效冲突的既有样式
    /// （GPUI `Styled` refinement 不可移除、不可读回，属框架限制）；
    /// 建议将动画应用于包装元素而非直接动画元素本身。
    ///
    /// 输入钳制（I8：Fade/Expand 钳制到 `[0, 1]`，Slide* 仅拒绝负值）：
    /// - `Fade` / `Expand*`：钳制到 `[0, 1]`（透明度过冲无视觉意义、尺寸不允许越过目标）；
    /// - `Slide*`：仅拒绝负值，允许 `> 1` 过冲（位移过冲有效果）。
    ///
    /// 通常不需要手动调用——[`Animated<T>`][crate::Animated] 在渲染时自动调用此方法。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use gpui_component_motion::Motion;
    /// use gpui::{div, px, Styled};
    ///
    /// // t=0：透明
    /// let _el = Motion::Fade.apply(div(), 0.0);
    /// // t=1：完全不透明
    /// let _el = Motion::Fade.apply(div(), 1.0);
    /// // t=0.5：宽度为最大值的一半
    /// let _el = Motion::ExpandWidth(px(200.)).apply(div(), 0.5);
    /// ```
    pub fn apply<E: Styled>(&self, el: E, t: f32) -> E {
        match self {
            // Fade：纯透明度，钳 [0, 1]
            Motion::Fade => el.opacity(clamp_unit(t)),
            // Slide*：纯位移，从偏移位置滑动到正常位置；允许过冲、拒绝负值
            Motion::SlideUp(offset) => el.top(lerp_px(*offset, px(0.0), max0(t))),
            Motion::SlideDown(offset) => el.top(lerp_px(px(0.0) - *offset, px(0.0), max0(t))),
            Motion::SlideLeft(offset) => el.left(lerp_px(*offset, px(0.0), max0(t))),
            Motion::SlideRight(offset) => el.left(lerp_px(px(0.0) - *offset, px(0.0), max0(t))),
            // ExpandWidth：宽度从 0 到 max_width，布局渐变而非瞬变。
            // 注意：每帧改变尺寸会触发 taffy 子树重排与文本重排版，大文本子树慎用。
            Motion::ExpandWidth(max_width) => el.w(lerp_px(px(0.0), *max_width, clamp_unit(t))),
            // ExpandHeight：高度从 0 到 max_height，垂直布局渐变（同上重排成本）。
            Motion::ExpandHeight(max_height) => el.h(lerp_px(px(0.0), *max_height, clamp_unit(t))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::div;

    /// `apply` 仅依赖 `Styled`，不涉及窗口/上下文，
    /// 构造一个 `Div` 验证方法可编译并返回元素即可。
    #[test]
    fn apply_fade_to_div() {
        let _el = Motion::Fade.apply(div(), 0.5);
    }

    #[test]
    fn apply_slide_up_to_div() {
        let _el = Motion::SlideUp(px(10.0)).apply(div(), 0.5);
    }

    #[test]
    fn apply_slide_variants() {
        for motion in [
            Motion::SlideDown(px(8.0)),
            Motion::SlideLeft(px(8.0)),
            Motion::SlideRight(px(8.0)),
        ] {
            let _el = motion.apply(div(), 0.0);
            let _el = motion.apply(div(), 1.0);
        }
    }

    #[test]
    fn apply_expand_width_to_div() {
        let _el = Motion::ExpandWidth(px(100.0)).apply(div(), 0.5);
    }

    #[test]
    fn apply_expand_height_to_div() {
        let _el = Motion::ExpandHeight(px(100.0)).apply(div(), 0.5);
    }

    #[test]
    fn motion_is_copy() {
        let m = Motion::Fade;
        let m2 = m;
        // Copy 语义：两者应相等
        assert_eq!(m, m2);
    }

    /// T6：`clamp_unit` / `max0` 边界（0 / 1 / 负 / 超 1）。
    #[test]
    fn clamp_unit_boundaries() {
        assert_eq!(clamp_unit(0.0), 0.0);
        assert_eq!(clamp_unit(1.0), 1.0);
        assert_eq!(clamp_unit(-0.5), 0.0);
        assert_eq!(clamp_unit(1.5), 1.0);
    }

    #[test]
    fn max0_boundaries() {
        assert_eq!(max0(0.0), 0.0);
        assert_eq!(max0(1.0), 1.0);
        assert_eq!(max0(-0.5), 0.0);
        assert_eq!(max0(1.5), 1.5);
    }
}
