//! 关键帧动画（Phase 1 / B4）：将一段总时长拆分为多段子动画顺序播放。
//!
//! [`MotionKeyframes<T>`] 包装一个 `IntoElement + Styled` 元素，通过
//! [`gpui::AnimationExt::with_animations`] 挂载多段 [`gpui::Animation`]：
//! 每段按 [`Keyframe`] 的 `ratio` 权重分配时长，并在自己的段内独立完成该段
//! [`Motion`] 的 from → to 过渡——GPUI 的 `AnimationState` 按段序号顺序推进。
//!
//! GPUI 的 `AnimationElement` 已内置 `reduce_motion` 检测：启用时直接渲染
//! 结束帧，无需在此重复处理（同 [`crate::Animated`] 的既有模式）。

use std::time::Duration;

use gpui::{Animation, AnimationExt, AnyElement, ElementId, IntoElement, ParentElement, Styled};

use crate::{Easing, Motion, spec::MIN_ANIMATION_DURATION};

/// 单个关键帧：一段子动画的时长权重、缓动曲线与动效。
///
/// `ratio` 是相对权重而非绝对比例——构建时按全部帧的 `ratio` 归一化
/// （总和 ≠ 1 时按比例缩放，总和 = 0 时均匀分），各帧只需保持相对大小。
///
/// 每段 [`Motion`] 在**自己的段内**完成 from → to 过渡（如 `Fade` 在该段内
/// 从 `0` 到 `1`）；同一样式属性的多态关键帧（如 opacity 0 → 1 → 0.5）
/// 不在本版支持，留待后续扩展。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Keyframe {
    /// 该段在总时长中的相对权重（非归一化，构建时按全部帧的比例缩放）。
    pub ratio: f32,
    /// 该段使用的缓动曲线，输出恒 ∈ [0, 1]（GPUI `Animation` 的 `debug_assert` 约束）。
    pub easing: Easing,
    /// 该段使用的动效，在该段内从起始态过渡到终态。
    pub motion: Motion,
}

/// 空 `frames` 时的兜底帧：单段 Fade / Linear / ratio 1.0。
const FALLBACK_FRAME: Keyframe = Keyframe {
    ratio: 1.0,
    easing: Easing::Linear,
    motion: Motion::Fade,
};

/// 多段关键帧动画包装器：将总时长按 [`Keyframe`] 序列拆分为多段子动画顺序播放。
///
/// 分段规则：`ratio` 归一化后每段时长 = 总时长 × 归一化 ratio，且每段至少
/// 1ms（`MIN_ANIMATION_DURATION`）；末段吸收舍入余数，使总时长精确等于
/// `max(total, 1ms)`。总时长为 0 时按 1ms 处理。
///
/// **语义**：每段动画的 animator 收到该段缓动后的进度 `t ∈ [0, 1]`，
/// 以 `t.clamp(0.0, 1.0)` 调用 [`Motion::apply`]——每段 [`Motion`] 在自己的
/// 段内完成 from → to（如 `Fade` = 0 → 1）。段与段之间**不共享样式进度**：
/// 同一样式属性的多态关键帧（如 opacity 0 → 1 → 0.5）不在本版支持，
/// 留待后续扩展。
///
/// 未添加任何关键帧时（`frames` 为空），回退为单段
/// `Keyframe { ratio: 1.0, easing: Linear, motion: Fade }`，等价于一次普通淡入。
///
/// 实现 [`ParentElement`]：子元素在动画应用前附着到被包装元素
/// （对齐 S12，与 [`crate::Animated`] 一致）。
///
/// # 示例
///
/// ```ignore
/// use gpui_component_motion::{Easing, Motion, MotionKeyframes};
/// use gpui::{div, px, Styled};
/// use std::time::Duration;
///
/// // 总时长 300ms，按 ratio 1:2 拆成两段：100ms 淡入 + 200ms 上滑
/// let el = MotionKeyframes::new(div(), "demo", Duration::from_millis(300))
///     .keyframe(1.0, Easing::EaseOut, Motion::Fade)
///     .keyframe(2.0, Easing::EaseIn, Motion::SlideUp(px(10.0)));
/// ```
pub struct MotionKeyframes<T: IntoElement + Styled + 'static> {
    inner: T,
    id: ElementId,
    total: Duration,
    frames: Vec<Keyframe>,
}

impl<T: IntoElement + Styled + 'static> MotionKeyframes<T> {
    /// 创建一个多段关键帧动画包装器。
    ///
    /// `total` 为动画总时长（所有段时长之和，为 0 时按 1ms 处理）；
    /// `id` 为该动画元素的唯一标识——同一父元素下多个动画元素需各自唯一，
    /// 否则 GPUI 会复用动画状态导致异常。`frames` 初始为空，渲染时回退为
    /// 单段 Fade（见 [`Self`] 文档）。
    pub fn new(inner: T, id: impl Into<ElementId>, total: Duration) -> Self {
        Self {
            inner,
            id: id.into(),
            total,
            frames: Vec::new(),
        }
    }

    /// 追加一段关键帧。
    ///
    /// `ratio` 为相对权重，构建时按全部帧归一化（总和 ≠ 1 时按比例缩放，
    /// 总和 = 0 时均匀分）；`easing` 决定该段缓动（输出恒 ∈ [0, 1]），
    /// `motion` 决定该段动效。每段时长 = 总时长 × 归一化 ratio，
    /// 且每段至少 1ms（`MIN_ANIMATION_DURATION`）。
    pub fn keyframe(mut self, ratio: f32, easing: Easing, motion: Motion) -> Self {
        self.frames.push(Keyframe {
            ratio,
            easing,
            motion,
        });
        self
    }
}

/// 空 `frames` 时替换为兜底帧；非空时原样返回（复制，`Keyframe` 为 `Copy`）。
fn effective_frames(frames: &[Keyframe]) -> Vec<Keyframe> {
    if frames.is_empty() {
        vec![FALLBACK_FRAME]
    } else {
        frames.to_vec()
    }
}

/// 计算各段时长（分段规则见 [`MotionKeyframes`] 文档）。
fn segment_durations(total: Duration, frames: &[Keyframe]) -> Vec<Duration> {
    let frames = effective_frames(frames);
    // total 为 0 时钳到 MIN_ANIMATION_DURATION
    let target = total.max(MIN_ANIMATION_DURATION);
    // 用纳秒 + f64 计算，避免 f32 秒换算的 ±1ns 舍入误差（常见比例可精确分配）
    let target_ns = target.as_nanos() as f64;

    // 归一化：负值 / NaN 权重按 0 处理；总和 ≤ 0 时均匀分
    let sum: f32 = frames.iter().map(|f| f.ratio.max(0.0)).sum();
    let normalized: Vec<f32> = if sum > f32::EPSILON {
        frames.iter().map(|f| f.ratio.max(0.0) / sum).collect()
    } else {
        vec![1.0 / frames.len() as f32; frames.len()]
    };

    // 每段时长 = 总时长 × 归一化 ratio，且每段至少 MIN_ANIMATION_DURATION
    let mut durations: Vec<Duration> = normalized
        .into_iter()
        .map(|r| {
            Duration::from_nanos((target_ns * r as f64).round() as u64).max(MIN_ANIMATION_DURATION)
        })
        .collect();

    // 末段吸收余数，使总时长精确等于 max(total, MIN)；
    // 若每段下限之和已超过该目标（段数过多），总时长取下限之和。
    let last = durations.len() - 1;
    let allocated: Duration = durations.iter().sum();
    durations[last] += target.saturating_sub(allocated);
    durations
}

/// 按帧序列构建 GPUI `Animation` 列表（每段一个）。
fn build_animations(total: Duration, frames: &[Keyframe]) -> Vec<Animation> {
    let frames = effective_frames(frames);
    segment_durations(total, &frames)
        .into_iter()
        .zip(frames.iter())
        .map(|(seg_dur, kf)| {
            // easing 为 Copy，按值复制进闭包以满足 'static
            let easing = kf.easing;
            Animation::new(seg_dur).with_easing(move |t| easing.at(t))
        })
        .collect()
}

impl<T: IntoElement + Styled + 'static> IntoElement for MotionKeyframes<T> {
    type Element = gpui::AnimationElement<T>;

    fn into_element(self) -> Self::Element {
        let MotionKeyframes {
            inner,
            id,
            total,
            frames,
        } = self;
        let frames = if frames.is_empty() {
            vec![FALLBACK_FRAME]
        } else {
            frames
        };
        let animations = build_animations(total, &frames);
        inner.with_animations(id, animations, move |el, ix, t| {
            // ix 恒 < animations.len() == frames.len()（GPUI 按段序号顺序推进）
            frames[ix].motion.apply(el, t.clamp(0.0, 1.0))
        })
    }
}

/// 子元素在动画应用前附着到被包装元素（S12：`MotionKeyframes` 支持 ParentElement）。
impl<T: IntoElement + Styled + ParentElement + 'static> ParentElement for MotionKeyframes<T> {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.inner.extend(elements);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{div, px};
    use std::time::Duration;

    fn kf(ratio: f32, easing: Easing, motion: Motion) -> Keyframe {
        Keyframe {
            ratio,
            easing,
            motion,
        }
    }

    /// 归一化：[0.5, 0.5] 两段时长相等，且总和精确等于总时长。
    #[test]
    fn normalization_equal_ratios_split_evenly() {
        let frames = [
            kf(0.5, Easing::Linear, Motion::Fade),
            kf(0.5, Easing::Linear, Motion::Fade),
        ];
        let durs = segment_durations(Duration::from_millis(100), &frames);
        assert_eq!(durs.len(), 2);
        assert_eq!(durs[0], durs[1]);
        assert_eq!(durs[0] + durs[1], Duration::from_millis(100));
    }

    /// 归一化：[1.0] 单段，时长 = 总时长。
    #[test]
    fn single_full_ratio_single_segment() {
        let frames = [kf(1.0, Easing::Linear, Motion::Fade)];
        let durs = segment_durations(Duration::from_millis(150), &frames);
        assert_eq!(durs.len(), 1);
        assert_eq!(durs[0], Duration::from_millis(150));
    }

    /// 归一化：总和 ≠ 1 时按比例缩放（1:3 → 1/4 与 3/4）。
    #[test]
    fn ratios_scaled_when_sum_not_one() {
        let frames = [
            kf(1.0, Easing::Linear, Motion::Fade),
            kf(3.0, Easing::Linear, Motion::Fade),
        ];
        let durs = segment_durations(Duration::from_millis(400), &frames);
        assert_eq!(durs[0], Duration::from_millis(100));
        assert_eq!(durs[1], Duration::from_millis(300));
        assert_eq!(durs[0] + durs[1], Duration::from_millis(400));
    }

    /// 归一化：总和 = 0 时均匀分。
    #[test]
    fn zero_ratios_split_uniformly() {
        let frames = [
            kf(0.0, Easing::Linear, Motion::Fade),
            kf(0.0, Easing::Linear, Motion::Fade),
        ];
        let durs = segment_durations(Duration::from_millis(200), &frames);
        assert_eq!(durs[0], Duration::from_millis(100));
        assert_eq!(durs[1], Duration::from_millis(100));
    }

    /// total = 0 时总时长钳到 `MIN_ANIMATION_DURATION`。
    #[test]
    fn zero_total_clamped_to_min() {
        let frames = [kf(1.0, Easing::Linear, Motion::Fade)];
        let durs = segment_durations(Duration::ZERO, &frames);
        assert_eq!(durs.len(), 1);
        assert_eq!(durs[0], MIN_ANIMATION_DURATION);
    }

    /// 每段至少 `MIN_ANIMATION_DURATION`：1ms 塞 3 段 → 每段被抬到下限，
    /// 总时长取下限之和（无法低于下限）。
    #[test]
    fn every_segment_at_least_min_duration() {
        let frames = [
            kf(1.0, Easing::Linear, Motion::Fade),
            kf(1.0, Easing::Linear, Motion::Fade),
            kf(1.0, Easing::Linear, Motion::Fade),
        ];
        let durs = segment_durations(Duration::from_millis(1), &frames);
        for d in &durs {
            assert!(*d >= MIN_ANIMATION_DURATION, "段时长 {d:?} 低于下限");
        }
        assert_eq!(
            durs.iter().sum::<Duration>(),
            Duration::from_millis(3),
            "总时长应为每段下限之和"
        );
    }

    /// 每段 easing 输出恒 ∈ [0, 1]（0..=100 采样）。
    #[test]
    fn easing_output_in_unit_range() {
        for easing in [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
        ] {
            let frames = [kf(0.5, easing, Motion::Fade), kf(0.5, easing, Motion::Fade)];
            let anims = build_animations(Duration::from_millis(200), &frames);
            assert_eq!(anims.len(), 2);
            for (ix, anim) in anims.iter().enumerate() {
                for i in 0..=100 {
                    let t = i as f32 / 100.0;
                    let v = (anim.easing)(t);
                    assert!(
                        (0.0..=1.0).contains(&v),
                        "{easing:?} 段 {ix} t={t} 输出 {v} 超出 [0, 1]"
                    );
                }
            }
        }
    }

    /// 空 `frames` 兜底：视为单段 Fade / Linear / ratio 1.0，可编译且产出单段动画。
    #[test]
    fn empty_frames_fallback_single_fade() {
        // 兜底帧内容
        let fallback = effective_frames(&[]);
        assert_eq!(fallback, vec![kf(1.0, Easing::Linear, Motion::Fade)]);
        // 兜底构建出单段动画，时长 = 总时长
        let anims = build_animations(Duration::from_millis(120), &[]);
        assert_eq!(anims.len(), 1);
        assert_eq!(anims[0].duration, Duration::from_millis(120));
        // 空 frames 的 MotionKeyframes 可直接 into_element（编译验证）
        let el = MotionKeyframes::new(div(), "fallback", Duration::from_millis(120)).into_element();
        let _ = el;
    }

    /// 编译测试：builder 链 + ParentElement 链（child / children）可编译，
    /// `into_element` 产出 `gpui::AnimationElement<T>`。
    #[test]
    fn into_element_and_parent_chain_compile() {
        let el = MotionKeyframes::new(div(), "kf", Duration::from_millis(300))
            .keyframe(1.0, Easing::EaseOut, Motion::Fade)
            .keyframe(2.0, Easing::EaseIn, Motion::SlideUp(px(10.0)))
            .child(div().child("a"))
            .children([div().child("b"), div().child("c")]);
        let _: gpui::AnimationElement<gpui::Div> = el.into_element();
    }

    /// 编译测试：与 `.with_animation` 等价的链式用法（构造 → 动画 → 子元素 → 渲染）。
    #[test]
    fn with_animation_style_chain_compiles() {
        let el = MotionKeyframes::new(div(), "chain", Duration::from_millis(200))
            .keyframe(0.5, Easing::EaseOut, Motion::ExpandWidth(px(100.0)))
            .keyframe(0.5, Easing::Linear, Motion::Fade)
            .child("text");
        let _ = el.into_element();
    }
}
