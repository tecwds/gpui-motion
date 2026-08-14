//! 缓动函数与 Spring 物理动画。
//!
//! [`Easing`] 为传统数学缓动曲线（二次函数），输出严格 `[0, 1]`，
//! 受 GPUI `Animation` 的 `debug_assert` 约束。
//!
//! [`SpringPreset`] 为基于阻尼振荡方程的物理动画曲线，
//! 输出可超过 `1`（过冲），在 [`crate::Animated`] 层接管而非走 GPUI easing，
//! 因此不受 `[0, 1]` 约束限制。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::LazyLock;
use std::time::Duration;

use gpui::Animation;

/// 常用缓动曲线。
///
/// `at(t)` 接收 `[0, 1]` 区间的线性进度并返回 `[0, 1]` 区间的缓动进度。
/// 输出必须落在 `[0, 1]` 内——这是 GPUI `Animation` 内部 `debug_assert` 的约束。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Easing {
    /// 线性：`t` 原样返回。
    Linear,
    /// 二次缓入：开始慢、结束快。适合退场动画。
    EaseIn,
    /// 二次缓出：开始快、结束慢。适合入场动画。
    EaseOut,
    /// 二次缓入缓出：两端慢、中间快。适合位置过渡。
    EaseInOut,
}

impl Easing {
    /// 根据线性进度 `t`（`[0, 1]`）计算缓动后的进度。
    ///
    /// 当 `t` 越界时自动钳制到 `[0, 1]`，保证返回值始终落在该区间。
    ///
    /// # 示例
    ///
    /// ```
    /// use gpui_component_motion::Easing;
    ///
    /// assert_eq!(Easing::Linear.at(0.5), 0.5);
    /// assert_eq!(Easing::EaseIn.at(0.5), 0.25);
    /// assert!((Easing::EaseOut.at(0.5) - 0.75).abs() < 1e-6);
    /// ```
    pub fn at(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t,
            Easing::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
        }
    }
}

/// Spring 物理动画预设。
///
/// 基于阻尼振荡方程的闭式解，通过阻尼比 ζ 和自然频率 ω₀ 控制曲线形态：
///
/// - **临界 / 过阻尼**（ζ ≥ 1）：单调递增至 1，无过冲。
/// - **欠阻尼**（ζ < 1）：先冲过 1 再回落振荡，过冲量随 ζ 减小而增大。
///
/// 与 [`Easing`] 不同，`curve(t)` 的输出**可以超过 1**（过冲），
/// 因此不能直接作为 GPUI `Animation` 的 easing 函数
/// （GPUI 内部 `debug_assert` 要求输出 ∈ `[0, 1]`）。
/// [`crate::Animated`] 在 Spring 模式下将 GPUI easing 设为 `Linear`
/// （原样传递线性进度），在 animator 回调内部调用 `curve(t)` 做物理映射。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpringPreset {
    /// 临界阻尼（ζ=1.0, ω₀=35）：无过冲，快速稳定。适合退场、折叠。
    Stiff,
    /// 默认（ζ=0.7, ω₀=30）：轻微过冲。通用入场。
    Default,
    /// 温和（ζ=0.5, ω₀=25）：温和过冲。适合大面积过渡。
    Gentle,
    /// 弹跳（ζ=0.3, ω₀=28）：明显振荡。适合活泼的 pop 效果。
    Wobbly,
}

impl SpringPreset {
    /// 返回本预设的预计算常量（P4）。
    fn constants(&self) -> &'static SpringConstants {
        &SPRING_TABLE[*self as usize]
    }

    /// 推荐动画时长：基于截止时间自动推导。
    ///
    /// 调用 [`crate::AnimationSpec::with_spring`] 时自动设置为此值。
    ///
    /// # 示例
    ///
    /// ```
    /// use gpui_component_motion::SpringPreset;
    ///
    /// // Stiff（临界阻尼）收敛最快，时长最短
    /// assert!(SpringPreset::Stiff.recommended_duration()
    ///     < SpringPreset::Default.recommended_duration());
    /// // Wobbly（低阻尼）振荡最久，时长最长
    /// assert!(SpringPreset::Default.recommended_duration()
    ///     < SpringPreset::Wobbly.recommended_duration());
    /// ```
    pub fn recommended_duration(&self) -> Duration {
        Duration::from_secs_f32(self.constants().cutoff)
    }

    /// Spring 曲线：输入归一化线性进度 `t`（`[0, 1]`），
    /// 返回 Spring 物理位置（可能 > 1，即过冲）。
    ///
    /// - `t = 0` → `0`（起始位置）
    /// - `t = 1` → `≈ 1`（稳定位置，残差 < 2%）
    /// - 中间可能 > 1（欠阻尼过冲）
    ///
    /// 常量（ζ、ω、截断时间、ω_d、ζω）已在 `SPRING_TABLE` 预计算（P4），
    /// 每样本仅做 `exp` / `cos` / `sin` 与线性组合。
    ///
    /// # 示例
    ///
    /// ```
    /// use gpui_component_motion::SpringPreset;
    ///
    /// // 起点为零
    /// assert!(SpringPreset::Default.curve(0.0).abs() < 1e-5);
    /// // 终点接近 1（残差 < 2%）
    /// assert!((1.0 - SpringPreset::Default.curve(1.0)).abs() < 0.02);
    /// // 欠阻尼预设中间会过冲（> 1）
    /// let max = (0..=1000)
    ///     .map(|i| SpringPreset::Wobbly.curve(i as f32 / 1000.0))
    ///     .fold(0.0_f32, f32::max);
    /// assert!(max > 1.0);
    /// ```
    pub fn curve(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let c = self.constants();
        let pt = t * c.cutoff;

        if c.zeta >= 1.0 {
            // 临界阻尼：x(t) = 1 - e^(-ω₀t) * (1 + ω₀t)
            let e = (-c.omega * pt).exp();
            1.0 - e * (1.0 + c.omega * pt)
        } else {
            // 欠阻尼：x(t) = 1 - e^(-ζω₀t) * (cos(ωdt) + (ζω₀/ωd) * sin(ωdt))
            let decay = (-c.zeta * c.omega * pt).exp();
            let cos = (c.omega_d * pt).cos();
            let sin = (c.omega_d * pt).sin();
            1.0 - decay * (cos + (c.zeta_omega / c.omega_d) * sin)
        }
    }
}

/// Spring 预设的预计算常量（P4）。
///
/// 截断时间的推导：
/// - 临界 / 过阻尼（ζ ≥ 1）：衰减包络为 `e^(-ω₀t) * (1 + ω₀t)`，
///   比纯指数慢，需要 `ω₀t ≈ 6.5` 才能使残差 < 2%。
/// - 欠阻尼（ζ < 1）：衰减包络为纯指数 `e^(-ζω₀t)`，
///   `ζω₀t ≈ 4.6`（ln 100）即可使残差 < 1%。
struct SpringConstants {
    /// 阻尼比 ζ。
    zeta: f32,
    /// 自然频率 ω₀。
    omega: f32,
    /// 截断时间：Spring 曲线归一化到 `[0, 1]` 时间域时对应的物理时间。
    cutoff: f32,
    /// 阻尼振荡频率 ω_d = ω₀·√(1-ζ²)（欠阻尼分支）。
    omega_d: f32,
    /// ζ·ω₀（衰减指数系数）。
    zeta_omega: f32,
}

impl SpringConstants {
    fn new(zeta: f32, omega: f32) -> Self {
        Self {
            zeta,
            omega,
            cutoff: if zeta >= 1.0 {
                6.5 / omega
            } else {
                4.605_17 / (zeta * omega)
            },
            // `f32::sqrt` 在 const 上下文中不可用（rustc 1.96 仍未 const-stable），
            // 故整表用 `LazyLock` 一次性初始化（spec P4 允许 once_cell/static 方案）。
            omega_d: omega * (1.0 - zeta * zeta).sqrt(),
            zeta_omega: zeta * omega,
        }
    }
}

/// 按预设索引的常量表（P4）：`curve(t)` 每样本只做 `exp`/`cos`/`sin` + 线性组合。
///
/// 常量与旧版逐点公式逐位一致（见 T16），初始化后只读、零每帧开销。
static SPRING_TABLE: LazyLock<[SpringConstants; 4]> = LazyLock::new(|| {
    [
        SpringConstants::new(1.0, 35.0), // Stiff（临界阻尼）
        SpringConstants::new(0.7, 30.0), // Default
        SpringConstants::new(0.5, 25.0), // Gentle
        SpringConstants::new(0.3, 28.0), // Wobbly
    ]
});

// === P1：`Animation` 按规格键的 Rc 缓存 ===

/// 动画缓存键：完全由公开输入决定（时长、延迟、缓动、弹簧、方向）。
///
/// `spring` 为 `Option`——`None`（传统缓动模式）与任一预设生成的 easing
/// 闭包不同，必须参与键区分。
pub(crate) type AnimKey = (Duration, Duration, Easing, Option<SpringPreset>, bool);

/// 缓存容量上限：超出后停止插入新键（命中既有键仍有效），防止无界增长。
const ANIM_CACHE_CAP: usize = 128;

thread_local! {
    /// 主线程 UI 的动画缓存：`thread_local!` 无锁、无需 `Send`/`Sync`。
    static ANIM_CACHE: RefCell<HashMap<AnimKey, Rc<Animation>>> = RefCell::new(HashMap::new());
}

/// 全局动画缓存访问器（P1）。
///
/// gpui 为主线程 UI 框架，动画构建只发生在主线程，因此采用
/// `thread_local! + RefCell<HashMap>`——无锁、零开销；键完全由公开输入
/// 决定（`Duration` 有界），无生命周期问题。容量上限 `ANIM_CACHE_CAP`。
pub(crate) struct AnimationCache;

impl AnimationCache {
    /// 命中返回缓存的同一 `Rc`（`Rc::ptr_eq` 成立）；未命中调用 `build`
    /// 构建并插入缓存（不超过 `ANIM_CACHE_CAP` 条）。
    pub(crate) fn get_or_insert(
        &self,
        key: AnimKey,
        build: impl FnOnce() -> Rc<Animation>,
    ) -> Rc<Animation> {
        if let Some(anim) = ANIM_CACHE.with(|cell| cell.borrow().get(&key).cloned()) {
            return anim;
        }
        let anim = build();
        ANIM_CACHE.with(|cell| {
            let mut cache = cell.borrow_mut();
            if cache.len() < ANIM_CACHE_CAP {
                cache.insert(key, anim.clone());
            }
        });
        anim
    }
}

/// 全局动画缓存句柄（P1）。
pub(crate) static ANIMATION_CACHE: AnimationCache = AnimationCache;

/// 弹簧进度：`t >= 1.0` 时精确返回 `1.0`（覆盖 GPUI 末帧钉住与 `reduce_motion`
/// 直达终态路径，S2：t≥1 精确返回 1.0），否则返回 [`SpringPreset::curve`] 值（保留过冲）
/// 并防御性钳制负值。
///
/// 仅用于入场方向；退场经 [`crate::AnimationSpec::without_spring`] 强制剥离 Spring。
pub(crate) fn spring_progress(preset: SpringPreset, t: f32) -> f32 {
    if t >= 1.0 {
        1.0
    } else {
        preset.curve(t).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T4：`t >= 1.0` 时所有预设均精确返回 `1.0`（覆盖 GPUI 末帧钉住与 reduce_motion 路径）。
    #[test]
    fn spring_progress_snaps_at_one() {
        for preset in [
            SpringPreset::Stiff,
            SpringPreset::Default,
            SpringPreset::Gentle,
            SpringPreset::Wobbly,
        ] {
            assert_eq!(
                spring_progress(preset, 1.0),
                1.0,
                "{preset:?} at t=1.0 should snap to exactly 1.0"
            );
        }
    }

    /// T5：`t < 1.0` 时与 `curve(t).max(0.0)` 逐点一致，过冲保留。
    #[test]
    fn spring_progress_preserves_overshoot() {
        for preset in [
            SpringPreset::Stiff,
            SpringPreset::Default,
            SpringPreset::Gentle,
            SpringPreset::Wobbly,
        ] {
            for i in 0..1000 {
                let t = i as f32 / 1000.0;
                assert_eq!(
                    spring_progress(preset, t),
                    preset.curve(t).max(0.0),
                    "{preset:?} at {t} should match curve"
                );
            }
        }
        for preset in [
            SpringPreset::Default,
            SpringPreset::Gentle,
            SpringPreset::Wobbly,
        ] {
            let max_val = (0..1000)
                .map(|i| spring_progress(preset, i as f32 / 1000.0))
                .fold(0.0_f32, f32::max);
            assert!(
                max_val > 1.0,
                "{preset:?} should preserve overshoot (max > 1.0), got max={max_val}"
            );
        }
    }

    /// 所有缓动在 `t = 0` 时必须返回 `0`。
    #[test]
    fn easing_at_zero() {
        for easing in [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
        ] {
            assert_eq!(easing.at(0.0), 0.0, "{easing:?} at 0.0 should be 0.0");
        }
    }

    /// 所有缓动在 `t = 1` 时必须返回 `1`。
    #[test]
    fn easing_at_one() {
        for easing in [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
        ] {
            assert_eq!(easing.at(1.0), 1.0, "{easing:?} at 1.0 should be 1.0");
        }
    }

    /// 所有缓动在 `[0, 1]` 上必须单调不减。
    #[test]
    fn easing_monotonic() {
        let steps = 100;
        for easing in [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
        ] {
            let mut prev = 0.0;
            for i in 1..=steps {
                let t = i as f32 / steps as f32;
                let v = easing.at(t);
                assert!(
                    v >= prev - f32::EPSILON,
                    "{easing:?} not monotonic at t={t}: {v} < {prev}"
                );
                prev = v;
            }
        }
    }

    /// 所有缓动返回值必须落在 `[0, 1]` 内。
    #[test]
    fn easing_in_range() {
        let steps = 200;
        for easing in [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
        ] {
            for i in 0..=steps {
                let t = i as f32 / steps as f32;
                let v = easing.at(t);
                assert!(
                    (0.0..=1.0).contains(&v),
                    "{easing:?} at {t} = {v}, out of [0, 1]"
                );
            }
        }
    }

    /// 钳制越界输入。
    #[test]
    fn easing_clamps_input() {
        assert_eq!(Easing::Linear.at(-0.5), 0.0);
        assert_eq!(Easing::Linear.at(1.5), 1.0);
    }

    // === SpringPreset 测试 ===

    /// Spring 曲线在 t=0 时必须返回 0。
    #[test]
    fn spring_curve_at_zero() {
        for preset in [
            SpringPreset::Stiff,
            SpringPreset::Default,
            SpringPreset::Gentle,
            SpringPreset::Wobbly,
        ] {
            let v = preset.curve(0.0);
            assert!(v.abs() < 1e-5, "{preset:?} at 0.0 should be ~0.0, got {v}");
        }
    }

    /// Spring 曲线在 t=1 时必须接近 1（1% 衰减截断）。
    #[test]
    fn spring_curve_at_one() {
        for preset in [
            SpringPreset::Stiff,
            SpringPreset::Default,
            SpringPreset::Gentle,
            SpringPreset::Wobbly,
        ] {
            let v = preset.curve(1.0);
            assert!(
                (1.0 - v).abs() < 0.02,
                "{preset:?} at 1.0 should be ~1.0 (within 2%), got {v}"
            );
        }
    }

    /// 欠阻尼 Spring（Default / Gentle / Wobbly）必须有过冲（> 1）。
    #[test]
    fn spring_overshoots_for_underdamped() {
        for preset in [
            SpringPreset::Default,
            SpringPreset::Gentle,
            SpringPreset::Wobbly,
        ] {
            let max_val = (0..=1000)
                .map(|i| preset.curve(i as f32 / 1000.0))
                .fold(0.0_f32, f32::max);
            assert!(
                max_val > 1.0,
                "{preset:?} should overshoot (max > 1.0), got max={max_val}"
            );
        }
    }

    /// 临界阻尼 Spring（Stiff）不得过冲（始终 ≤ 1）。
    #[test]
    fn spring_stiff_no_overshoot() {
        for i in 0..=1000 {
            let t = i as f32 / 1000.0;
            let v = SpringPreset::Stiff.curve(t);
            assert!(v <= 1.0 + 1e-5, "Stiff at {t} = {v}, should not overshoot");
        }
    }

    /// 推荐时长必须为正数。
    #[test]
    fn spring_recommended_duration_positive() {
        for preset in [
            SpringPreset::Stiff,
            SpringPreset::Default,
            SpringPreset::Gentle,
            SpringPreset::Wobbly,
        ] {
            assert!(
                preset.recommended_duration().as_secs_f32() > 0.0,
                "{preset:?} recommended duration should be positive"
            );
        }
    }

    /// Wobbly 的过冲量应大于 Default（阻尼比更小 → 振荡更剧烈）。
    #[test]
    fn spring_wobbly_overshoots_more_than_default() {
        let wobbly_max = (0..=1000)
            .map(|i| SpringPreset::Wobbly.curve(i as f32 / 1000.0))
            .fold(0.0_f32, f32::max);
        let default_max = (0..=1000)
            .map(|i| SpringPreset::Default.curve(i as f32 / 1000.0))
            .fold(0.0_f32, f32::max);
        assert!(
            wobbly_max > default_max,
            "Wobbly overshoot ({wobbly_max}) should exceed Default ({default_max})"
        );
    }

    /// T16（P4）：常量表驱动的新 `curve` 与旧版逐点公式数值完全一致
    /// （保护既有 4 个 preset 的曲线行为）。
    #[test]
    fn spring_constants_match_legacy_curve() {
        fn legacy_curve(preset: SpringPreset, t: f32) -> f32 {
            let t = t.clamp(0.0, 1.0);
            let (zeta, omega) = match preset {
                SpringPreset::Stiff => (1.0, 35.0),
                SpringPreset::Default => (0.7, 30.0),
                SpringPreset::Gentle => (0.5, 25.0),
                SpringPreset::Wobbly => (0.3, 28.0),
            };
            let cutoff = if zeta >= 1.0 {
                6.5 / omega
            } else {
                4.605_17 / (zeta * omega)
            };
            let pt = t * cutoff;
            if zeta >= 1.0 {
                let e = (-omega * pt).exp();
                1.0 - e * (1.0 + omega * pt)
            } else {
                let omega_d = omega * (1.0 - zeta * zeta).sqrt();
                let decay = (-zeta * omega * pt).exp();
                let cos = (omega_d * pt).cos();
                let sin = (omega_d * pt).sin();
                1.0 - decay * (cos + (zeta * omega / omega_d) * sin)
            }
        }

        for preset in [
            SpringPreset::Stiff,
            SpringPreset::Default,
            SpringPreset::Gentle,
            SpringPreset::Wobbly,
        ] {
            for i in 0..=1000 {
                let t = i as f32 / 1000.0;
                assert_eq!(
                    preset.curve(t),
                    legacy_curve(preset, t),
                    "{preset:?} at t={t}: 常量表曲线与旧公式不一致"
                );
            }
        }
    }
}
