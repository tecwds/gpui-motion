//! 动画包装器：将一个元素与动画规格、预设动效绑定。
//!
//! `Animated<T>` 实现了 [`gpui::IntoElement`]，内部通过
//! [`gpui::AnimationExt::with_animation`] 委托给 GPUI 的 `AnimationElement`。
//! GPUI 的 `AnimationElement` 已内置 `reduce_motion` 检测——
//! 启用时直接渲染结束帧，无需在此重复处理。

use gpui::{Animation, AnimationExt, AnyElement, ElementId, IntoElement, ParentElement, Styled};

use crate::{AnimationSpec, Motion, easing::spring_progress, spec::MIN_ANIMATION_DURATION};

/// 预构建 GPUI `Animation`：`total` 钳制到 [`MIN_ANIMATION_DURATION`]（S1 零时长防护），
/// 缓动闭包在构建期一次性分配（`Rc` 浅拷贝复用，见 S9）。
///
/// `reverse = true` 时输出反向缓动（`1 → 0`，用于退场）。
pub(crate) fn build_animation(spec: AnimationSpec, reverse: bool) -> Animation {
    let total = (spec.duration + spec.delay).max(MIN_ANIMATION_DURATION);
    let delay_ratio = spec.delay_ratio();
    let easing = spec.easing;
    let spring = spec.spring;

    // GPUI easing 闭包：始终输出 [0, 1] 以满足 GPUI 的 debug_assert 约束。
    // - Spring 模式：输出线性进度（delay 后原样传递），
    //   物理映射在 animator 回调中由 spring_progress 完成。
    // - 传统模式：输出 easing 曲线值（入场 0→1 / 退场 1→0）。
    Animation::new(total).with_easing(move |t| {
        let progress = if t < delay_ratio {
            0.0
        } else if delay_ratio >= 1.0 {
            1.0
        } else {
            (t - delay_ratio) / (1.0 - delay_ratio)
        };
        if spring.is_some() {
            progress
        } else {
            let raw = easing.at(progress);
            if reverse { 1.0 - raw } else { raw }
        }
    })
}

/// 持有一个 `IntoElement + Styled` 元素及其动画规格，
/// 渲染时自动应用入场 / 退场动画。
///
/// 通常不直接构造，而是通过 [`MotionExt`][crate::MotionExt] 的快捷方法
/// （`fade_in` / `slide_up` / …）创建，再用 builder 方法链式定制。
///
/// # 示例
///
/// ```ignore
/// use gpui_component_motion::{MotionExt, AnimationSpec, Easing, Motion};
/// use gpui::{div, px, Styled};
/// use std::time::Duration;
///
/// // 通过 MotionExt 创建，再链式定制
/// let el = div()
///     .fade_in("my-fade")
///     .with_spec(AnimationSpec::default().with_duration(Duration::from_millis(400)))
///     .with_motion(Motion::SlideUp(px(10.0)));
/// ```
pub struct Animated<T: IntoElement + Styled + 'static> {
    pub(crate) inner: T,
    pub(crate) spec: AnimationSpec,
    pub(crate) motion: Motion,
    pub(crate) id: ElementId,
    /// `false` = 入场（`0 → 1`）；`true` = 退场（`1 → 0`）。
    pub(crate) reverse: bool,
    /// 退场动效，默认等于 `motion`。
    pub(crate) exit_motion: Motion,
    /// 退场规格，默认等于 `spec` 剥离 Spring 后的结果。
    pub(crate) exit_spec: AnimationSpec,
    /// 预构建的入场动画（构建期分配缓动闭包，渲染期仅浅拷贝 `Rc`）。
    pub(crate) enter_animation: Animation,
    /// 预构建的退场动画。
    pub(crate) exit_animation: Animation,
}

impl<T: IntoElement + Styled + 'static> Animated<T> {
    /// 创建一个新的 `Animated` 包装器（默认入场模式）。
    ///
    /// 退场规格由 `spec.without_spring()` 派生——退场强制剥离 Spring
    /// （退场过冲到负值对 width / opacity 无意义），若原规格的时长是 Spring
    /// 推荐时长且未显式覆盖，则重置为默认 200ms。
    pub fn new(inner: T, id: impl Into<ElementId>, spec: AnimationSpec, motion: Motion) -> Self {
        let exit_spec = spec.without_spring();
        Self {
            enter_animation: build_animation(spec, false),
            exit_animation: build_animation(exit_spec, true),
            exit_motion: motion,
            exit_spec,
            inner,
            spec,
            motion,
            id: id.into(),
            reverse: false,
        }
    }

    /// 替换入场动画规格（时长 / 延迟 / 缓动 / Spring）。
    ///
    /// 恒作用于入场阶段（退场用 [`with_exit`](Self::with_exit)）；
    /// 退场规格不受影响，除非后续调用 [`with_exit`](Self::with_exit)。
    pub fn with_spec(mut self, spec: AnimationSpec) -> Self {
        self.spec = spec;
        self.enter_animation = build_animation(spec, false);
        self
    }

    /// 替换入场预设动效（如从 `Fade` 改为 `SlideUp`）。
    ///
    /// 恒作用于入场阶段（退场用 [`with_exit`](Self::with_exit)）。
    pub fn with_motion(mut self, motion: Motion) -> Self {
        self.motion = motion;
        self
    }

    /// 替换元素 ID。
    ///
    /// 同一父元素下多个动画子元素需各自唯一，否则 GPUI 会复用动画状态导致异常。
    pub fn with_id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = id.into();
        self
    }

    /// 配对退场动效与规格。未调用则退场使用入场动效的反向缓动。
    ///
    /// 传入的规格会经 `without_spring()` 剥离 Spring（退场恒不使用 Spring，
    /// 若时长等于 Spring 推荐时长则重置为默认 200ms，否则保留显式时长）。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use gpui_component_motion::{MotionExt, AnimationSpec, Motion};
    /// use gpui::{div, px, Styled};
    /// use std::time::Duration;
    ///
    /// // 入场 SlideUp，退场 Fade（更快）
    /// let el = div()
    ///     .slide_up("panel", px(20.0))
    ///     .with_exit(Motion::Fade, AnimationSpec::fast());
    /// ```
    pub fn with_exit(mut self, motion: Motion, spec: AnimationSpec) -> Self {
        let spec = spec.without_spring();
        self.exit_motion = motion;
        self.exit_spec = spec;
        self.exit_animation = build_animation(spec, true);
        self
    }

    /// 切换为退场模式：缓动反向（`1 → 0`），元素从可见过渡到隐藏。
    ///
    /// 入场时需预设 `t=0` 起始态以避免首帧闪现最终态；
    /// 退场时元素默认可见（`t=1`），由反向缓动驱动至 `t=0`。
    ///
    /// 通常不直接调用——[`PresenceState`][crate::PresenceState] 在退场阶段自动调用。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use gpui_component_motion::{Animated, AnimationSpec, Motion};
    /// use gpui::{div, Styled};
    ///
    /// let el = Animated::new(div(), "exit-demo", AnimationSpec::default(), Motion::Fade)
    ///     .exit(); // 现在是退场动画
    /// ```
    pub fn exit(mut self) -> Self {
        self.reverse = true;
        self
    }
}

impl<T: IntoElement + Styled + 'static> IntoElement for Animated<T> {
    type Element = gpui::AnimationElement<T>;

    fn into_element(self) -> Self::Element {
        // 退场使用 exit_motion / exit_animation，入场使用 motion / enter_animation。
        // 最终防线：reverse 时 spring 恒视为 None（S3.4）——退场在任何路径下都
        // 不使用 Spring（Gpui 动画不支持自定义起始进度，反向 Spring 会过冲负值）。
        let (motion, animation, spring) = if self.reverse {
            (self.exit_motion, self.exit_animation, None)
        } else {
            (self.motion, self.enter_animation, self.spec.spring)
        };

        // 预设起始态：入场 t=0（隐藏），退场 t=1（可见）。
        let initial_t = if self.reverse { 1.0 } else { 0.0 };
        let inner = motion.apply(self.inner, initial_t);

        // animator 回调：每帧由 GPUI 调用，t 是 easing 闭包的输出值。
        // 预构建的 Animation 在此仅做 `clone()`（Rc 浅拷贝，零堆分配）。
        inner.with_animation(self.id, animation, move |el, t| {
            let actual_t = match spring {
                // Spring 物理映射：t>=1 精确返回 1（终态精确），t<1 保留过冲。
                Some(preset) => spring_progress(preset, t),
                // 传统模式：t 已是 easing 后的值。
                None => t,
            };
            motion.apply(el, actual_t)
        })
    }
}

/// 子元素在动画应用前附着到被包装元素（S12）。
impl<T: IntoElement + Styled + ParentElement + 'static> ParentElement for Animated<T> {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.inner.extend(elements);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ext::MotionExt;
    use gpui::{div, px};
    use std::time::Duration;

    #[test]
    fn default_exit_equals_enter() {
        let a = Animated::new(div(), "id", AnimationSpec::default(), Motion::Fade);
        assert_eq!(a.exit_motion, a.motion);
        assert_eq!(a.exit_spec, a.spec);
    }

    #[test]
    fn with_exit_overrides_exit_only() {
        let a = div()
            .slide_up("id", px(10.0))
            .with_exit(Motion::Fade, AnimationSpec::fast());
        assert_eq!(a.motion, Motion::SlideUp(px(10.0)));
        assert_eq!(a.spec, AnimationSpec::default());
        assert_eq!(a.exit_motion, Motion::Fade);
        assert_eq!(a.exit_spec, AnimationSpec::fast());
    }

    #[test]
    fn exit_sets_reverse_flag() {
        let a = div().fade_in("id").exit();
        assert!(a.reverse);
    }

    #[test]
    fn with_exit_then_exit_uses_exit_motion() {
        // 验证 into_element 在 reverse 时选择 exit_motion / exit_animation
        let a = div()
            .fade_in("id")
            .with_exit(Motion::SlideUp(px(20.0)), AnimationSpec::slow())
            .exit();
        assert!(a.reverse);
        assert_eq!(a.exit_motion, Motion::SlideUp(px(20.0)));
        assert_eq!(a.exit_spec, AnimationSpec::slow());
        // 触发 into_element 验证可编译
        let _ = a.into_element();
    }

    /// T1：零总时长被钳制到 `MIN_ANIMATION_DURATION`。
    #[test]
    fn total_clamped_when_zero() {
        let zero = AnimationSpec::default()
            .with_duration(Duration::ZERO)
            .with_delay(Duration::ZERO);
        assert_eq!(
            build_animation(zero, false).duration,
            MIN_ANIMATION_DURATION
        );
    }

    /// T7：`build_animation` 输出恒 ∈ [0, 1]、duration > 0（零 total 与纯 delay 两边界）。
    #[test]
    fn build_animation_output_in_range_and_positive_duration() {
        let zero = AnimationSpec::default()
            .with_duration(Duration::ZERO)
            .with_delay(Duration::ZERO);

        // 零 total：duration 被钳制为正，easing 输出恒在 [0, 1]
        for reverse in [false, true] {
            let anim = build_animation(zero, reverse);
            assert!(anim.duration > Duration::ZERO);
            for i in 0..=100 {
                let v = (anim.easing)(i as f32 / 100.0);
                assert!((0.0..=1.0).contains(&v), "reverse={reverse} t={i} v={v}");
            }
        }

        // 纯 delay：延迟期输出 0（起始态），t=1 输出 1（终态）
        let pure_delay = AnimationSpec::default()
            .with_duration(Duration::ZERO)
            .with_delay(Duration::from_millis(200));
        let anim = build_animation(pure_delay, false);
        for i in 0..100 {
            assert_eq!((anim.easing)(i as f32 / 100.0), 0.0, "delay 期应输出 0");
        }
        assert_eq!((anim.easing)(1.0), 1.0, "t=1 应输出终态");
    }

    /// T8：`with_exit` 传入 Spring 规格时，退场规格被强制剥离 Spring。
    #[test]
    fn exit_spec_never_has_spring() {
        let a = div().fade_in("id").with_exit(
            Motion::Fade,
            AnimationSpec::default().with_spring(crate::SpringPreset::Wobbly),
        );
        assert_eq!(a.exit_spec.spring, None);
        // 未显式覆盖时长 → 重置为默认 200ms
        assert_eq!(a.exit_spec.duration, AnimationSpec::default().duration);

        // 显式覆盖时长 → 时长保留
        let a = div().fade_in("id").with_exit(
            Motion::Fade,
            AnimationSpec::default()
                .with_spring(crate::SpringPreset::Wobbly)
                .with_duration(Duration::from_millis(600)),
        );
        assert_eq!(a.exit_spec.spring, None);
        assert_eq!(a.exit_spec.duration, Duration::from_millis(600));
    }

    /// T13：`Animated` 实现 `ParentElement`，`div().fade_in("x").child(...)` 可编译。
    #[test]
    fn animated_parent_chain() {
        let el = div()
            .fade_in("x")
            .child(div().child("child"))
            .children([div().child("a"), div().child("b")]);
        let _ = el;
    }

    /// S12 行为：子元素附着到被包装元素。
    #[test]
    fn animated_extend_reaches_inner() {
        let mut a = div().fade_in("x");
        let child = div().child("c").into_any_element();
        a.extend(std::iter::once(child));
        let _ = a.into_element();
    }
}
