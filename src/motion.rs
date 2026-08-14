//! 预设动画效果。
//!
//! 每个 [`Motion`] 变体描述一种入场视觉效果，
//! 通过 [`Motion::apply`] 将缓动进度 `t`（`0 → 1`）映射到具体样式。
//!
//! 仅使用 `Styled` 提供的 `opacity` / `top` / `left` / `width` / `height`
//! 实现位移、淡入与尺寸展开，并用 `bg` / `text_color` / `border_color`
//! 实现颜色插值（`BackgroundColor` / `TextColor` / `BorderColor`）——
//! GPUI 的 `Styled` 不支持 `scale` / `transform`，因此 `ScaleIn` / `Pop`
//! 等缩放类效果无法在不自定义 `Element` 的前提下实现，留待后续阶段。

use gpui::{Hsla, Pixels, Styled, px};

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

/// 对 `Hsla` 颜色做插值：`t = 0` 返回 `from`，`t = 1` 返回 `to`。
///
/// 内部先将 `t` 钳制到 `[0, 1]`；`s` / `l` / `a` 线性插值，
/// `h`（色相）走最短路径：必要时跨 `0/1` 环绕边界插值，
/// 避免绕色环长路导致中间色相偏离直觉（如红→青直穿而非绕行）。
pub(crate) fn lerp_hsla(from: Hsla, to: Hsla, t: f32) -> Hsla {
    let t = t.clamp(0.0, 1.0);
    let s = from.s + (to.s - from.s) * t;
    let l = from.l + (to.l - from.l) * t;
    let a = from.a + (to.a - from.a) * t;
    // 色相最短路径：delta ∈ (-0.5, 0.5]，指向最近的环绕方向。
    let mut delta = (to.h - from.h).rem_euclid(1.0);
    if delta > 0.5 {
        delta -= 1.0;
    }
    let h = (from.h + delta * t).rem_euclid(1.0);
    Hsla { h, s, l, a }
}

/// 预设入场动画。
///
/// 变体携带的 `Pixels` 参数表示位移幅度（Slide*）或目标尺寸（Expand*）；
/// 颜色变体（`BackgroundColor` / `TextColor` / `BorderColor`）携带一对
/// `Hsla`，语义为 `from → to` 的渐变。
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
    /// 背景色插值：`background` 从 `from` 渐变到 `to`。
    BackgroundColor(Hsla, Hsla),
    /// 文字颜色插值：`color` 从 `from` 渐变到 `to`。
    TextColor(Hsla, Hsla),
    /// 边框颜色插值：`border_color` 从 `from` 渐变到 `to`。
    BorderColor(Hsla, Hsla),
}

impl Motion {
    /// 将缓动进度 `t`（`0 → 1`，`0` 为起始状态、`1` 为结束状态）
    /// 应用到元素 `el` 上，返回应用了样式的元素。
    ///
    /// `t = 0` 时元素处于入场前状态（透明 / 偏移 / 零宽高 / 起始颜色）。
    /// `t = 1` 时应用 Motion 的**终态样式**，覆盖元素上与该动效冲突的既有样式
    /// （GPUI `Styled` refinement 不可移除、不可读回，属框架限制）；
    /// 建议将动画应用于包装元素而非直接动画元素本身。
    ///
    /// 输入钳制（I8：Fade/Expand/颜色 钳制到 `[0, 1]`，Slide* 仅拒绝负值）：
    /// - `Fade` / `Expand*` / 颜色变体：钳制到 `[0, 1]`（透明度过冲无视觉意义、
    ///   尺寸不允许越过目标、颜色不过冲）；
    /// - `Slide*`：仅拒绝负值，允许 `> 1` 过冲（位移过冲有效果）。
    ///
    /// 通常不需要手动调用——[`Animated<T>`][crate::Animated] 在渲染时自动调用此方法。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use gpui_component_motion::Motion;
    /// use gpui::{div, px, hsla, Styled};
    ///
    /// // t=0：透明
    /// let _el = Motion::Fade.apply(div(), 0.0);
    /// // t=1：完全不透明
    /// let _el = Motion::Fade.apply(div(), 1.0);
    /// // t=0.5：宽度为最大值的一半
    /// let _el = Motion::ExpandWidth(px(200.)).apply(div(), 0.5);
    /// // t=0.5：背景色渐变到中点
    /// let _el = Motion::BackgroundColor(hsla(0.0, 1.0, 0.5, 1.0), hsla(0.5, 1.0, 0.5, 1.0))
    ///     .apply(div(), 0.5);
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
            // 颜色插值：色相走最短路径、其余通道线性插值；钳 [0, 1]，不过冲
            Motion::BackgroundColor(from, to) => el.bg(lerp_hsla(*from, *to, clamp_unit(t))),
            Motion::TextColor(from, to) => el.text_color(lerp_hsla(*from, *to, clamp_unit(t))),
            Motion::BorderColor(from, to) => el.border_color(lerp_hsla(*from, *to, clamp_unit(t))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{div, hsla};

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

    /// `lerp_hsla` 端点：`t=0` 返回 `from`，`t=1` 返回 `to`（逐字段断言）。
    #[test]
    fn lerp_hsla_endpoints() {
        let from = hsla(0.0, 0.1, 0.2, 0.3);
        let to = hsla(0.5, 0.6, 0.7, 0.8);

        let at_zero = lerp_hsla(from, to, 0.0);
        assert!(
            (at_zero.h - from.h).abs() < 1e-5,
            "h: {} != {}",
            at_zero.h,
            from.h
        );
        assert!(
            (at_zero.s - from.s).abs() < 1e-5,
            "s: {} != {}",
            at_zero.s,
            from.s
        );
        assert!(
            (at_zero.l - from.l).abs() < 1e-5,
            "l: {} != {}",
            at_zero.l,
            from.l
        );
        assert!(
            (at_zero.a - from.a).abs() < 1e-5,
            "a: {} != {}",
            at_zero.a,
            from.a
        );

        let at_one = lerp_hsla(from, to, 1.0);
        assert!(
            (at_one.h - to.h).abs() < 1e-5,
            "h: {} != {}",
            at_one.h,
            to.h
        );
        assert!(
            (at_one.s - to.s).abs() < 1e-5,
            "s: {} != {}",
            at_one.s,
            to.s
        );
        assert!(
            (at_one.l - to.l).abs() < 1e-5,
            "l: {} != {}",
            at_one.l,
            to.l
        );
        assert!(
            (at_one.a - to.a).abs() < 1e-5,
            "a: {} != {}",
            at_one.a,
            to.a
        );
    }

    /// 色相走最短路径：`from.h=0.9` → `to.h=0.1` 的捷径跨 `0/1` 环绕边界，
    /// 中点色相应 ≈ `0.0`（落在边界），而非不环绕的长路中点 `0.5`。
    #[test]
    fn lerp_hsla_hue_shortest_path() {
        let from = hsla(0.9, 0.5, 0.5, 1.0);
        let to = hsla(0.1, 0.5, 0.5, 1.0);
        let mid = lerp_hsla(from, to, 0.5);
        assert!(mid.h < 1e-4, "mid hue should wrap through 0, got {}", mid.h);

        // 反向亦然：0.1 → 0.9 的最短路径是退回跨 0 边界，中点同样落在环绕边界。
        // 环绕边界处 0.0 ≡ 1.0（同一色相）；f32 舍入误差可能落向任一侧，故两侧都接受。
        let from_rev = hsla(0.1, 0.5, 0.5, 1.0);
        let to_rev = hsla(0.9, 0.5, 0.5, 1.0);
        let mid_rev = lerp_hsla(from_rev, to_rev, 0.5);
        assert!(
            mid_rev.h < 1e-4 || mid_rev.h > 1.0 - 1e-4,
            "mid hue should wrap through the boundary, got {}",
            mid_rev.h
        );
    }

    /// 同色相场景下 `s` / `l` / `a` 线性插值，中点为两者均值。
    #[test]
    fn lerp_hsla_midpoint_linear() {
        let from = hsla(0.5, 0.2, 0.3, 0.4);
        let to = hsla(0.5, 0.6, 0.7, 0.8);
        let mid = lerp_hsla(from, to, 0.5);
        assert!((mid.h - 0.5).abs() < 1e-5, "h: {}", mid.h);
        assert!((mid.s - 0.4).abs() < 1e-5, "s: {}", mid.s);
        assert!((mid.l - 0.5).abs() < 1e-5, "l: {}", mid.l);
        assert!((mid.a - 0.6).abs() < 1e-5, "a: {}", mid.a);
    }

    /// `lerp_hsla` 对 `t < 0` / `t > 1` 钳制到端点。
    #[test]
    fn lerp_hsla_clamps_t() {
        let from = hsla(0.2, 0.1, 0.2, 0.3);
        let to = hsla(0.4, 0.5, 0.6, 0.7);

        let below = lerp_hsla(from, to, -1.0);
        assert!((below.h - from.h).abs() < 1e-5);
        assert!((below.s - from.s).abs() < 1e-5);
        assert!((below.l - from.l).abs() < 1e-5);
        assert!((below.a - from.a).abs() < 1e-5);

        let above = lerp_hsla(from, to, 2.0);
        assert!((above.h - to.h).abs() < 1e-5);
        assert!((above.s - to.s).abs() < 1e-5);
        assert!((above.l - to.l).abs() < 1e-5);
        assert!((above.a - to.a).abs() < 1e-5);
    }

    /// 三个颜色变体可应用到 `gpui::div()`（编译测试）。
    #[test]
    fn apply_background_color_to_div() {
        let _el = Motion::BackgroundColor(hsla(0.0, 0.0, 0.0, 1.0), hsla(0.5, 1.0, 0.5, 1.0))
            .apply(div(), 0.5);
    }

    #[test]
    fn apply_text_color_to_div() {
        let _el =
            Motion::TextColor(hsla(0.0, 0.0, 0.0, 1.0), hsla(0.5, 1.0, 0.5, 1.0)).apply(div(), 0.5);
    }

    #[test]
    fn apply_border_color_to_div() {
        let _el = Motion::BorderColor(hsla(0.0, 0.0, 0.0, 1.0), hsla(0.5, 1.0, 0.5, 1.0))
            .apply(div(), 0.5);
    }
}
