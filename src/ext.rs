//! `MotionExt` trait：为任意 `IntoElement + Styled` 元素添加动画快捷方法。
//!
//! 所有方法都需要一个 `ElementId`——这是 GPUI `with_animation` 的要求，
//! 用于在帧间保持动画状态。同一父元素下若有多个动画子元素，
//! 需为各自传入不同的 ID。
//!
//! # 一致性限制
//!
//! [`MotionExt`] 通过 blanket impl（`impl<E: IntoElement + Styled + 'static> MotionExt for E`）
//! 为所有满足约束的类型自动实现。这意味着：
//!
//! - **下游 crate 无法再为具体类型实现 `MotionExt`**（孤儿规则下 trait 已全局实现）；
//! - **方法名全局抢占**：`fade_in` / `slide_up` / `with_motion` 等方法名在
//!   整个依赖图中只能由本 crate 提供，其他 crate 不得定义同名方法。
//!
//! 这属于有意设计——该 trait 定位为"一行接入"的全局扩展点，代价是命名空间的独占。

use gpui::{ElementId, IntoElement, Pixels, Styled};

use crate::{Animated, AnimationSpec, Motion};

/// 为元素添加入场动画的快捷方法。
///
/// 对所有实现了 `IntoElement + Styled + 'static` 的类型自动实现（blanket impl，
/// 一致性限制：下游无法再为具体类型实现本 trait，方法名在依赖图中全局独占）。
///
/// # 示例
///
/// ```ignore
/// use gpui_component_motion::MotionExt;
/// use gpui::{div, px, Styled};
///
/// div()
///     .child(div().fade_in("fade-element"))
///     .child(div().slide_up("slide-element", px(10.0)));
/// ```
pub trait MotionExt: IntoElement + Styled + Sized {
    /// 淡入：透明度 `0 → 1`，使用默认规格。
    ///
    /// 恒作用于入场阶段（退场用 [`Animated::with_exit`]）。
    fn fade_in(self, id: impl Into<ElementId>) -> Animated<Self> {
        Animated::new(self, id, AnimationSpec::default(), Motion::Fade)
    }

    /// 从下方滑入：`top` 从 `+offset` 到 `0`。
    ///
    /// 恒作用于入场阶段（退场用 [`Animated::with_exit`]）。
    fn slide_up(self, id: impl Into<ElementId>, offset: Pixels) -> Animated<Self> {
        Animated::new(self, id, AnimationSpec::default(), Motion::SlideUp(offset))
    }

    /// 从上方滑入：`top` 从 `-offset` 到 `0`。
    ///
    /// 恒作用于入场阶段（退场用 [`Animated::with_exit`]）。
    fn slide_down(self, id: impl Into<ElementId>, offset: Pixels) -> Animated<Self> {
        Animated::new(
            self,
            id,
            AnimationSpec::default(),
            Motion::SlideDown(offset),
        )
    }

    /// 从右侧滑入：`left` 从 `+offset` 到 `0`（用于右侧面板）。
    ///
    /// 恒作用于入场阶段（退场用 [`Animated::with_exit`]）。
    fn slide_left(self, id: impl Into<ElementId>, offset: Pixels) -> Animated<Self> {
        Animated::new(
            self,
            id,
            AnimationSpec::default(),
            Motion::SlideLeft(offset),
        )
    }

    /// 从左侧滑入：`left` 从 `-offset` 到 `0`（用于左侧面板）。
    ///
    /// 恒作用于入场阶段（退场用 [`Animated::with_exit`]）。
    fn slide_right(self, id: impl Into<ElementId>, offset: Pixels) -> Animated<Self> {
        Animated::new(
            self,
            id,
            AnimationSpec::default(),
            Motion::SlideRight(offset),
        )
    }

    /// 使用自定义 [`Motion`] 与 [`AnimationSpec`] 包装元素。
    ///
    /// 恒作用于入场阶段（退场用 [`Animated::with_exit`]）。
    /// 当预设快捷方法（`fade_in` / `slide_*`）不够灵活时使用。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use gpui_component_motion::{MotionExt, AnimationSpec, Motion};
    /// use gpui::{div, px, Styled};
    ///
    /// div().with_motion(
    ///     "custom-anim",
    ///     AnimationSpec::default(),
    ///     Motion::ExpandWidth(px(340.)),
    /// );
    /// ```
    fn with_motion(
        self,
        id: impl Into<ElementId>,
        spec: AnimationSpec,
        motion: Motion,
    ) -> Animated<Self> {
        Animated::new(self, id, spec, motion)
    }
}

/// blanket impl：为所有 `IntoElement + Styled + 'static` 类型提供 [`MotionExt`] 方法。
///
/// 一致性限制：下游不可再为具体类型实现本 trait，
/// 方法名在依赖图中全局抢占——属有意设计。
impl<E: IntoElement + Styled + 'static> MotionExt for E {}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{div, px};

    /// 验证扩展方法可链式调用并返回 `Animated`。
    #[test]
    fn fade_in_returns_animated() {
        let _animated = div().fade_in("test-fade");
    }

    #[test]
    fn slide_up_returns_animated() {
        let _animated = div().slide_up("test-slide", px(10.0));
    }

    #[test]
    fn with_motion_returns_animated() {
        let _animated = div().with_motion(
            "test-custom",
            AnimationSpec::fast(),
            Motion::SlideDown(px(5.0)),
        );
    }

    /// 验证 builder 方法可在 `Animated` 上链式调用。
    #[test]
    fn animated_builder_chain() {
        let _animated = div()
            .fade_in("test-builder")
            .with_spec(AnimationSpec::slow())
            .with_motion(Motion::SlideLeft(px(12.0)));
    }
}
