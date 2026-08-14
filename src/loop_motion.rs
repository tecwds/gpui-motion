//! 循环动效预设（Phase 2 / C7）：为元素挂载永不结束的循环动画。
//!
//! 提供两种预设：[`LoopKind::Pulse`] 平滑呼吸与 [`LoopKind::Skeleton`] 骨架屏闪烁，
//! 均基于 [`gpui::Animation::repeat_synced`] 构建——循环动画相位锁到 App 共享时钟，
//! 所有 `repeat_synced` 循环由同一次帧调度驱动（比各自 [`gpui::Animation::repeat`] 省）。
//!
//! # 关键约束：必须条件挂载
//!
//! `repeat_synced` 动画**永不结束、每帧 tick**：只要元素保持挂载，窗口就会以满刷新率
//! 持续重绘（对照 README「性能」节：常驻 `repeat()` 组件钉住整窗满帧重绘）。
//! 调用方必须**按需挂载**（E1/E2 教训）：
//!
//! - 仅在需要循环效果时挂载（如 `loading` / `pulsing` 状态标志）；
//! - 效果结束时立即卸载——**卸载即停**，动画状态随元素销毁，无后台残留；
//! - 空闲时不挂载循环元素。
//!
//! # 曲线映射
//!
//! 动画 easing 保持 Linear（easing 输出 = 相位 `t ∈ [0, 1)`，满足 GPUI 的
//! `debug_assert` 约束），曲线映射在 animator 内由 [`loop_opacity`] 完成——
//! 一个可单测的纯函数。
//!
//! # reduce_motion
//!
//! GPUI 的 `AnimationElement` 已内置 `reduce_motion` 检测：启用时循环动画渲染
//! 起始相位（低谷帧）且不调度动画帧，无需在此重复处理（同 [`crate::Animated`] 模式）。
//!
//! # 已知限制
//!
//! GPUI 暂无渐变位置样式（`background: linear-gradient` 类），Skeleton 为近似 shimmer
//! 的三角波闪烁，真 shimmer 留待上游支持。Pulse / Skeleton 均作用于元素整体透明度
//! （含子元素）。

use std::time::Duration;

use gpui::{Animation, AnimationExt, AnyElement, ElementId, IntoElement, ParentElement, Styled};

use crate::spec::MIN_ANIMATION_DURATION;

/// 循环动效类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopKind {
    /// 平滑呼吸：抛物线曲线，透明度 `0.4 → 1.0 → 0.4`
    /// （t=0 低谷、t=0.5 峰值、t=1 回到低谷）。
    Pulse,
    /// 骨架屏闪烁：三角波曲线，透明度 `0.5 → 1.0 → 0.5`
    /// （t=0 低谷、t=0.5 峰值、t=1 回到低谷）。
    ///
    /// 近似 shimmer——GPUI 暂无渐变位置样式，真 shimmer 留待上游支持。
    Skeleton,
}

/// 循环动效包装器：为元素挂载一个永不结束的循环动画。
///
/// 包装一个 `IntoElement + Styled` 元素，通过
/// [`gpui::AnimationExt::with_animation`] 挂载 [`gpui::Animation::repeat_synced`]
/// 循环动画；animator 每帧收到相位 `t ∈ [0, 1]`，由内部的 `loop_opacity` 按
/// [`LoopKind`] 映射为元素整体透明度。
///
/// **循环永不结束，必须条件挂载**（E1/E2 教训）：只要元素保持挂载，每帧都会 tick
/// 并驱动整窗重绘（对照 README「性能」节：常驻 `repeat()` 组件钉住整窗满帧重绘）。
/// 请仅在需要循环效果时挂载（如 `loading` / `pulsing` 状态标志），效果结束时立即
/// 卸载——**卸载即停**，动画状态随元素一起销毁，无后台残留。
///
/// Pulse / Skeleton 作用于**元素整体透明度（含子元素）**：透明度呼吸 / 闪烁作用于
/// 包装元素及其全部后代。Skeleton 为近似 shimmer 的三角波闪烁，GPUI 暂无渐变位置
/// 样式，真 shimmer 留待上游支持。
///
/// 实现 [`ParentElement`]：子元素在动画应用前附着到被包装元素（对齐 S12，
/// 与 [`crate::Animated`] 一致）。
///
/// # 示例
///
/// ```ignore
/// use gpui_component_motion::LoopMotion;
/// use gpui::{div, Styled};
/// use std::time::Duration;
///
/// // 骨架屏闪烁：仅在 loading 时挂载，加载完成立即卸载
/// let el = LoopMotion::skeleton(
///     div().child("placeholder"),
///     "skeleton",
///     Duration::from_millis(800),
/// );
/// ```
pub struct LoopMotion<T: IntoElement + Styled + 'static> {
    inner: T,
    id: ElementId,
    kind: LoopKind,
    /// 单次循环周期（零时长被钳制到 [`MIN_ANIMATION_DURATION`]，S1 零时长防护）。
    period: Duration,
}

impl<T: IntoElement + Styled + 'static> LoopMotion<T> {
    /// 创建一个循环动效包装器。
    ///
    /// `id` 为该动画元素的唯一标识——同一父元素下多个动画元素需各自唯一，
    /// 否则 GPUI 会复用动画状态导致异常。`period` 为单次循环周期（低谷 → 峰值 → 低谷）；
    /// 零时长被钳制到 1ms，避免 GPUI 对 synced 动画按周期取模时除零 / NaN。
    pub fn new(inner: T, id: impl Into<ElementId>, kind: LoopKind, period: Duration) -> Self {
        Self {
            inner,
            id: id.into(),
            kind,
            period: period.max(MIN_ANIMATION_DURATION),
        }
    }

    /// 便捷构造：[`LoopKind::Pulse`] 平滑呼吸（透明度 `0.4 → 1.0 → 0.4`）。
    pub fn pulse(inner: T, id: impl Into<ElementId>, period: Duration) -> Self {
        Self::new(inner, id, LoopKind::Pulse, period)
    }

    /// 便捷构造：[`LoopKind::Skeleton`] 骨架屏闪烁（透明度 `0.5 → 1.0 → 0.5`）。
    pub fn skeleton(inner: T, id: impl Into<ElementId>, period: Duration) -> Self {
        Self::new(inner, id, LoopKind::Skeleton, period)
    }
}

/// 将循环相位 `t`（∈ [0, 1]，GPUI easing 输出）映射为该时刻的元素透明度。
///
/// 曲线映射在 animator 内做（而非 easing）：动画 easing 保持 Linear，此处按
/// [`LoopKind`] 做波形映射——输入先钳制到 `[0, 1]`（超界输入安全），
/// 输出恒 ∈ [0, 1]：
///
/// - [`LoopKind::Pulse`]：抛物线 `p = 1 - (2t - 1)²`，`opacity = 0.4 + 0.6p`
///   （t=0 → 0.4，t=0.5 → 1.0，t=1 → 0.4，平滑呼吸）；
/// - [`LoopKind::Skeleton`]：三角波 `p = 1 - |2t - 1|`，`opacity = 0.5 + 0.5p`
///   （t=0 → 0.5，t=0.5 → 1.0，t=1 → 0.5，骨架屏闪烁）。
pub(crate) fn loop_opacity(kind: LoopKind, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match kind {
        LoopKind::Pulse => {
            let p = 1.0 - (2.0 * t - 1.0).powi(2);
            0.4 + 0.6 * p
        }
        LoopKind::Skeleton => {
            let p = 1.0 - (2.0 * t - 1.0).abs();
            0.5 + 0.5 * p
        }
    }
}

/// 构建循环动画：`Animation::new(period).repeat_synced()`。
///
/// `repeat_synced` 使动画循环播放且相位锁到 App 共享时钟——所有 `repeat_synced`
/// 循环由同一次帧调度驱动（比各自 `repeat()` 省）；easing 保持 Linear（输出即相位），
/// 曲线映射在 animator 内由 [`loop_opacity`] 完成。零时长被钳制到
/// [`MIN_ANIMATION_DURATION`]（S1 零时长防护）。
pub(crate) fn build_loop_animation(period: Duration) -> Animation {
    Animation::new(period.max(MIN_ANIMATION_DURATION)).repeat_synced()
}

impl<T: IntoElement + Styled + 'static> IntoElement for LoopMotion<T> {
    type Element = gpui::AnimationElement<T>;

    fn into_element(self) -> Self::Element {
        let Self {
            inner,
            id,
            kind,
            period,
        } = self;
        let animation = build_loop_animation(period);
        // kind 为 Copy 枚举，按值捕获进 animator 闭包（Animation 非 Copy，直接移动）。
        inner.with_animation(id, animation, move |el, t| {
            el.opacity(loop_opacity(kind, t))
        })
    }
}

/// 子元素在动画应用前附着到被包装元素（S12：`LoopMotion` 支持 ParentElement）。
impl<T: IntoElement + Styled + ParentElement + 'static> ParentElement for LoopMotion<T> {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.inner.extend(elements);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::div;
    use std::time::Duration;

    /// Pulse 曲线端点：t=0 → 0.4（低谷），t=0.5 → 1.0（峰值），t=1 → 0.4（回到低谷）。
    #[test]
    fn pulse_opacity_endpoints() {
        assert!((loop_opacity(LoopKind::Pulse, 0.0) - 0.4).abs() < 1e-6);
        assert!((loop_opacity(LoopKind::Pulse, 0.5) - 1.0).abs() < 1e-6);
        assert!((loop_opacity(LoopKind::Pulse, 1.0) - 0.4).abs() < 1e-6);
    }

    /// Skeleton 曲线端点：t=0 → 0.5（低谷），t=0.5 → 1.0（峰值），t=1 → 0.5（回到低谷）。
    #[test]
    fn skeleton_opacity_endpoints() {
        assert!((loop_opacity(LoopKind::Skeleton, 0.0) - 0.5).abs() < 1e-6);
        assert!((loop_opacity(LoopKind::Skeleton, 0.5) - 1.0).abs() < 1e-6);
        assert!((loop_opacity(LoopKind::Skeleton, 1.0) - 0.5).abs() < 1e-6);
    }

    /// 全程（0..=1 细采样）输出恒 ∈ [0, 1]（GPUI opacity 合法区间）。
    #[test]
    fn loop_opacity_in_unit_range() {
        for kind in [LoopKind::Pulse, LoopKind::Skeleton] {
            for i in 0..=1000 {
                let t = i as f32 / 1000.0;
                let o = loop_opacity(kind, t);
                assert!((0.0..=1.0).contains(&o), "{kind:?} t={t} opacity={o}");
            }
        }
    }

    /// `repeat_synced` 语义（E1/E2 教训的根基）：循环播放、锁 App 共享时钟、Linear easing。
    #[test]
    fn loop_animation_is_synced_repeating_linear() {
        let anim = build_loop_animation(Duration::from_millis(800));
        assert!(!anim.oneshot, "repeat_synced 应循环播放（oneshot = false）");
        assert!(
            anim.synced,
            "repeat_synced 应锁 App 共享时钟（synced = true）"
        );
        assert_eq!(anim.duration, Duration::from_millis(800));
        for i in 0..=100 {
            let t = i as f32 / 100.0;
            let v = (anim.easing)(t);
            assert!((t - v).abs() < 1e-6, "easing 应保持 Linear，输出 {v}");
        }
    }

    /// S1 零时长防护：零周期被钳制到 `MIN_ANIMATION_DURATION`（避免 GPUI 取模除零）。
    #[test]
    fn zero_period_clamped_to_min() {
        let anim = build_loop_animation(Duration::ZERO);
        assert_eq!(anim.duration, MIN_ANIMATION_DURATION);
        assert!(anim.duration > Duration::ZERO);
        // 构造器同样钳制
        let lm = LoopMotion::pulse(div(), "id", Duration::ZERO);
        assert_eq!(lm.period, MIN_ANIMATION_DURATION);
    }

    /// 编译测试：便捷构造 + `into_element` 产出 `gpui::AnimationElement<Div>`。
    #[test]
    fn into_element_compiles() {
        let el: gpui::AnimationElement<gpui::Div> =
            LoopMotion::pulse(div(), "id", Duration::from_millis(800)).into_element();
        let _ = el;
    }

    /// 编译测试：ParentElement 链（`child` / `children`）可编译，再 `into_element`。
    #[test]
    fn parent_chain_compiles() {
        let el = LoopMotion::skeleton(div(), "x", Duration::from_millis(800))
            .child(div().child("child"))
            .children([div().child("a"), div().child("b")]);
        let _ = el.into_element();
    }

    /// 便捷构造等价于 `new` + 对应 [`LoopKind`]。
    #[test]
    fn convenience_constructors_set_kind() {
        let p = LoopMotion::pulse(div(), "p", Duration::from_millis(800));
        assert_eq!(p.kind, LoopKind::Pulse);
        let s = LoopMotion::skeleton(div(), "s", Duration::from_millis(800));
        assert_eq!(s.kind, LoopKind::Skeleton);
        assert_eq!(p.period, Duration::from_millis(800));
        assert_eq!(s.period, Duration::from_millis(800));
    }
}
