//! 动画规格：时长、延迟与缓动的组合。

use std::time::Duration;

use crate::{Easing, SpringPreset};

/// 动画总时长（`duration + delay`）的最小值，防止零时长导致的 NaN / 除零路径。
pub(crate) const MIN_ANIMATION_DURATION: Duration = Duration::from_millis(1);

/// 描述一次动画的时长、延迟与缓动曲线。
///
/// `delay` 通过将缓动输入的前导 `delay/(delay+duration)` 区间映射为 `0` 实现，
/// 因此无需额外的计时器或帧调度——GPUI 的 `Animation` 基座即可处理。
///
/// `spring` 字段为 `Some` 时启用 Spring 物理动画：入场使用 Spring 曲线
/// （可能过冲），退场则经 [`without_spring`](Self::without_spring) 强制剥离
/// （S3 规则：退场剥离 Spring、未显式覆盖时重置 200ms；退场过冲到负值无意义）。
///
/// # 示例
///
/// ```
/// use gpui_component_motion::{AnimationSpec, Easing, SpringPreset};
/// use std::time::Duration;
///
/// // 预设
/// let fast = AnimationSpec::fast();       // 120ms EaseOut
/// let default = AnimationSpec::default(); // 200ms EaseOut
/// let slow = AnimationSpec::slow();       // 350ms EaseInOut
///
/// // Builder 链式定制
/// let custom = AnimationSpec::default()
///     .with_duration(Duration::from_millis(400))
///     .with_delay(Duration::from_millis(50))
///     .with_easing(Easing::EaseInOut);
///
/// // Spring 物理缓动（自动设置推荐时长）
/// let spring = AnimationSpec::default().with_spring(SpringPreset::Wobbly);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationSpec {
    /// 动画主体时长（不含延迟）。
    pub duration: Duration,
    /// 动画开始前的等待时长。在此期间元素停留在起始状态。
    pub delay: Duration,
    /// 缓动曲线。
    pub easing: Easing,
    /// Spring 物理动画预设。`Some` 时入场使用 Spring 曲线替代 `easing`。
    pub spring: Option<SpringPreset>,
}

impl AnimationSpec {
    /// 快速预设：120ms，无延迟，`EaseOut`。
    pub fn fast() -> Self {
        Self {
            duration: Duration::from_millis(120),
            delay: Duration::ZERO,
            easing: Easing::EaseOut,
            spring: None,
        }
    }

    /// 慢速预设：350ms，无延迟，`EaseInOut`。
    pub fn slow() -> Self {
        Self {
            duration: Duration::from_millis(350),
            delay: Duration::ZERO,
            easing: Easing::EaseInOut,
            spring: None,
        }
    }

    /// 设置时长。
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// 设置延迟。
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// 设置缓动曲线。
    pub fn with_easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    /// 启用 Spring 物理动画并自动设置推荐时长。
    ///
    /// 调用后入场动画使用 Spring 曲线（可能过冲），取代 `easing`。
    /// **覆盖先前设置的 `duration`**（设为该预设的推荐时长），
    /// 可之后链式调用 [`with_duration`](Self::with_duration) 再次覆盖。
    /// 退场仍使用 `easing`（退场经 [`without_spring`](Self::without_spring) 强制剥离 Spring）。
    ///
    /// # 示例
    ///
    /// ```
    /// use gpui_component_motion::{AnimationSpec, SpringPreset};
    /// use std::time::Duration;
    ///
    /// // 使用推荐时长
    /// let spec = AnimationSpec::default().with_spring(SpringPreset::Default);
    ///
    /// // 覆盖时长
    /// let spec = AnimationSpec::default()
    ///     .with_spring(SpringPreset::Wobbly)
    ///     .with_duration(Duration::from_millis(600));
    /// ```
    pub fn with_spring(mut self, preset: SpringPreset) -> Self {
        self.duration = preset.recommended_duration();
        self.spring = Some(preset);
        self
    }

    /// 移除 Spring 预设（用于构造退场规格）。
    ///
    /// - `spring` 恒置为 `None`；
    /// - 若原 `spring` 为 `Some` 且时长未被显式覆盖（仍等于该预设的推荐时长），
    ///   时长重置为 [`default`](Self::default) 的 200ms；
    /// - 否则时长 / 延迟 / 缓动保持不变。
    pub fn without_spring(mut self) -> Self {
        if let Some(preset) = self.spring.take()
            && self.duration == preset.recommended_duration()
        {
            self.duration = Self::default().duration;
        }
        self
    }

    /// `delay` 占 `delay + duration` 的比例，用于构造复合缓动函数。
    /// 当总时长为零时返回 `0.0`，避免除零。
    pub(crate) fn delay_ratio(&self) -> f32 {
        let total = self.delay + self.duration;
        if total.is_zero() {
            0.0
        } else {
            self.delay.as_secs_f32() / total.as_secs_f32()
        }
    }
}

/// 默认预设：200ms，无延迟，`EaseOut`。
impl Default for AnimationSpec {
    fn default() -> Self {
        Self {
            duration: Duration::from_millis(200),
            delay: Duration::ZERO,
            easing: Easing::EaseOut,
            spring: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_durations() {
        assert_eq!(AnimationSpec::fast().duration, Duration::from_millis(120));
        assert_eq!(
            AnimationSpec::default().duration,
            Duration::from_millis(200)
        );
        assert_eq!(AnimationSpec::slow().duration, Duration::from_millis(350));
    }

    #[test]
    fn preset_no_delay() {
        assert_eq!(AnimationSpec::fast().delay, Duration::ZERO);
        assert_eq!(AnimationSpec::default().delay, Duration::ZERO);
        assert_eq!(AnimationSpec::slow().delay, Duration::ZERO);
    }

    #[test]
    fn default_uses_ease_out() {
        assert_eq!(AnimationSpec::default().easing, Easing::EaseOut);
    }

    #[test]
    fn slow_uses_ease_in_out() {
        assert_eq!(AnimationSpec::slow().easing, Easing::EaseInOut);
    }

    #[test]
    fn delay_ratio_zero_when_no_delay() {
        let spec = AnimationSpec::default();
        assert_eq!(spec.delay_ratio(), 0.0);
    }

    #[test]
    fn delay_ratio_with_delay() {
        let spec = AnimationSpec::default()
            .with_delay(Duration::from_millis(100))
            .with_duration(Duration::from_millis(300));
        // delay=100ms, duration=300ms, total=400ms → 0.25
        assert!((spec.delay_ratio() - 0.25).abs() < 1e-6);
    }

    #[test]
    fn delay_ratio_zero_when_total_zero() {
        let spec = AnimationSpec::default()
            .with_delay(Duration::ZERO)
            .with_duration(Duration::ZERO);
        assert_eq!(spec.delay_ratio(), 0.0);
    }

    #[test]
    fn builder_chain() {
        let spec = AnimationSpec::default()
            .with_duration(Duration::from_millis(500))
            .with_delay(Duration::from_millis(50))
            .with_easing(Easing::Linear);
        assert_eq!(spec.duration, Duration::from_millis(500));
        assert_eq!(spec.delay, Duration::from_millis(50));
        assert_eq!(spec.easing, Easing::Linear);
    }

    /// T2：未显式覆盖时长时，`without_spring` 将时长重置为默认 200ms。
    #[test]
    fn without_spring_resets_auto_duration() {
        for preset in [
            SpringPreset::Stiff,
            SpringPreset::Default,
            SpringPreset::Gentle,
            SpringPreset::Wobbly,
        ] {
            let spring_spec = AnimationSpec::default().with_spring(preset);
            assert_eq!(
                spring_spec.duration,
                preset.recommended_duration(),
                "{preset:?} 应使用推荐时长"
            );
            let stripped = spring_spec.without_spring();
            assert_eq!(stripped.spring, None);
            assert_eq!(stripped.duration, AnimationSpec::default().duration);
            assert_eq!(stripped.delay, spring_spec.delay);
            assert_eq!(stripped.easing, spring_spec.easing);
        }
    }

    /// T3：显式覆盖时长后，`without_spring` 保留该时长。
    #[test]
    fn without_spring_keeps_explicit_duration() {
        let explicit = Duration::from_millis(600);
        let spec = AnimationSpec::default()
            .with_spring(SpringPreset::Wobbly)
            .with_duration(explicit)
            .with_delay(Duration::from_millis(40))
            .with_easing(Easing::EaseInOut);
        let stripped = spec.without_spring();
        assert_eq!(stripped.spring, None);
        assert_eq!(stripped.duration, explicit);
        assert_eq!(stripped.delay, spec.delay);
        assert_eq!(stripped.easing, spec.easing);
    }

    /// 无 Spring 的规格经 `without_spring` 后保持不变。
    #[test]
    fn without_spring_keeps_plain_spec_unchanged() {
        let spec = AnimationSpec::fast();
        assert_eq!(spec.without_spring(), spec);
    }
}
