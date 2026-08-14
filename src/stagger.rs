//! 级联动画辅助（Phase 1 / B5）：为一组 [`Animated`] 元素按索引递增延迟，形成级联入场。

use std::time::Duration;

use gpui::{IntoElement, Styled};

use crate::Animated;

/// 对一组 `Animated` 元素应用级联延迟（stagger），返回延迟调整后的新 `Vec`。
///
/// 第 `i` 个元素的动画延迟被**替换**为 `gap * i`（覆盖其原有的 `delay`）：
/// 第 0 个立即开始，第 1 个等待 `gap`，第 2 个等待 `2 * gap`，依此类推，
/// 元素按顺序依次入场，形成级联效果。乘法使用 [`Duration::saturating_mul`]，
/// 超大索引不会溢出。
///
/// 仅覆盖延迟，其余规格字段（`duration` / `easing` / `spring`）保持不变。
/// 适用于任何 `Animated` 元素——无论是 [`MotionExt::fade_in`](crate::MotionExt::fade_in) /
/// [`MotionExt::slide_up`](crate::MotionExt::slide_up) 还是
/// [`MotionExt::with_motion`](crate::MotionExt::with_motion) 的产物。
///
/// # 示例
///
/// ```ignore
/// use gpui_component_motion::{stagger, MotionExt};
/// use gpui::{div, Styled};
/// use std::time::Duration;
///
/// let items = vec![
///     div().fade_in("item-0"),
///     div().fade_in("item-1"),
///     div().fade_in("item-2"),
/// ];
/// // 第 i 个元素延迟 80ms * i：0ms / 80ms / 160ms
/// let staggered = stagger(items, Duration::from_millis(80));
/// ```
pub fn stagger<T: IntoElement + Styled + 'static>(
    elements: impl IntoIterator<Item = Animated<T>>,
    gap: Duration,
) -> Vec<Animated<T>> {
    elements
        .into_iter()
        .enumerate()
        .map(|(i, el)| {
            // 先读取原 spec 再消费 el：with_spec 会移动 el，延迟计算需在其之前完成。
            let spec = el.spec.with_delay(gap.saturating_mul(i as u32));
            el.with_spec(spec)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AnimationSpec, Easing, Motion, MotionExt, SpringPreset};
    use gpui::{div, px};
    use std::time::Duration;

    /// B5：第 i 个元素延迟为 `gap * i`（i=0 → ZERO，i=1 → gap，i=2 → 2*gap）。
    #[test]
    fn delays_are_gap_multiplied_by_index() {
        let gap = Duration::from_millis(80);
        let items = vec![div().fade_in("a"), div().fade_in("b"), div().fade_in("c")];
        let staggered = stagger(items, gap);

        assert_eq!(staggered.len(), 3);
        assert_eq!(staggered[0].spec.delay, Duration::ZERO, "i=0 延迟应为 ZERO");
        assert_eq!(staggered[1].spec.delay, gap, "i=1 延迟应为 gap");
        assert_eq!(
            staggered[2].spec.delay,
            gap.saturating_mul(2),
            "i=2 延迟应为 2*gap"
        );
    }

    /// B5：with_spec 后 duration / easing / spring 保持不变，仅 delay 被替换。
    #[test]
    fn other_spec_fields_preserved() {
        let gap = Duration::from_millis(100);
        let spec = AnimationSpec::default()
            .with_duration(Duration::from_millis(400))
            .with_easing(Easing::EaseInOut)
            .with_spring(SpringPreset::Gentle);
        let items = vec![
            div().with_motion("a", spec, Motion::SlideUp(px(10.0))),
            div().with_motion("b", spec, Motion::SlideUp(px(10.0))),
        ];
        let staggered = stagger(items, gap);

        for el in &staggered {
            assert_eq!(el.spec.duration, spec.duration, "duration 应保持不变");
            assert_eq!(el.spec.easing, spec.easing, "easing 应保持不变");
            assert_eq!(el.spec.spring, spec.spring, "spring 应保持不变");
        }
        assert_eq!(staggered[0].spec.delay, Duration::ZERO);
        assert_eq!(staggered[1].spec.delay, gap);
    }

    /// B5：对 `Vec<Animated<Div>>` 调用 stagger 可编译，且元素可 `into_element`。
    #[test]
    fn vec_of_animated_div_compiles_and_into_element() {
        let items: Vec<Animated<gpui::Div>> = vec![
            div().fade_in("a"),
            div().slide_up("b", px(10.0)),
            div().fade_in("c"),
        ];
        let staggered = stagger(items, Duration::from_millis(60));
        assert_eq!(staggered.len(), 3);
        // 元素可渲染：Animated<Div> 产物可转为 GPUI 元素
        for el in staggered {
            let _ = el.into_element();
        }
    }
}
