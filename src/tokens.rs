//! 组件动效令牌：为常见组件预设统一动效配对（零配置接入）。
//!
//! [`MotionTokens`] 把 [`Motion`] 与 [`AnimationSpec`] 的常用组合固化为
//! 命名预设（`panel` / `tooltip` / `modal` / `notification` / `dropdown` / `toast`），
//! 调用方无需记忆 `Motion` / `AnimationSpec` 的配对细节，一行即可拿到
//! 入场包装器（[`MotionTokens::animate`]）或完整生命周期配对
//! （[`MotionTokens::lifecycle`]）——把"一行接入"升级为"零配置接入"。
//!
//! # 预设对照表
//!
//! | 预设 | 组件场景 | 入场动效 | 入场规格 | 退场动效 | 退场规格 |
//! |------|----------|----------|----------|----------|----------|
//! | `panel` | 侧边栏 / 面板 | `ExpandWidth(340px)` | `default` + `Spring::Default` | `ExpandWidth(340px)` | `default` |
//! | `tooltip` | tooltip / 小浮层 | `Fade` | `fast`（120ms EaseOut） | `Fade` | `fast` |
//! | `modal` | 弹窗 | `Fade` | `default` + `Spring::Gentle` | `Fade` | `slow`（350ms EaseInOut） |
//! | `notification` | 通知条（右侧滑入） | `SlideLeft(40px)` | `fast` + 250ms | `SlideLeft(40px)` | `fast` |
//! | `dropdown` | 下拉 / 折叠 | `ExpandHeight(200px)` | `default` | `ExpandHeight(200px)` | `default` |
//! | `toast` | 轻提示（底部滑入） | `SlideUp(16px)` | `default` + `Spring::Default` | `SlideUp(16px)` | `fast` |
//!
//! # I3 防线：退场规格永远无 Spring
//!
//! 退场动画对尺寸 / 透明度过冲到负值无视觉意义，因此所有令牌的
//! `exit_spec` 恒不携带 Spring（I3：退场曲线严格单调）。预设直接构造为
//! 无 Spring 规格；`custom` 与 `with_exit` 传入的退场规格也会经
//! [`AnimationSpec::without_spring`] 强制剥离。
//!
//! # 示例
//!
//! ```ignore
//! use gpui_component_motion::MotionTokens;
//! use gpui::{div, Styled};
//!
//! // 入场包装：面板展开动画
//! let el = MotionTokens::panel().animate(div().child("content"), "panel");
//!
//! // 完整生命周期配对：交给 PresenceState 播放入场 / 退场
//! let lc = MotionTokens::modal().lifecycle();
//! ```
//!
//! 完全对齐 [`MotionLifecycle`] 的语义：`exit_spec` 与 `enter_spec` 独立，
//! 退场侧永不携带 Spring。

use std::time::Duration;

use gpui::{ElementId, IntoElement, Styled, px};

use crate::{Animated, AnimationSpec, Motion, MotionLifecycle, SpringPreset};

/// 常见组件的动效令牌：入场 / 退场动效与规格的命名预设。
///
/// 所有字段公开（`Copy`），可整体读取、替换或与 [`Self::custom`] 组合定制；
/// 不变式：`exit_spec` 恒不含 Spring（I3），由预设定义与
/// [`custom`](Self::custom) / [`with_exit`](Self::with_exit) 强制保证。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionTokens {
    /// 入场动效。
    pub enter: Motion,
    /// 入场规格（时长 / 延迟 / 缓动 / 可选 Spring 预设）。
    pub enter_spec: AnimationSpec,
    /// 退场动效。
    pub exit: Motion,
    /// 退场规格（时长 / 延迟 / 缓动；恒不含 Spring）。
    pub exit_spec: AnimationSpec,
}

impl MotionTokens {
    /// 侧边栏 / 面板：宽度展开入场（Spring 轻微过冲），退场宽度收缩。
    ///
    /// 入场 `ExpandWidth(340px)` + `default` + `Spring::Default`；
    /// 退场 `ExpandWidth(340px)` + `default`（无 Spring）。
    pub fn panel() -> Self {
        Self {
            enter: Motion::ExpandWidth(px(340.)),
            enter_spec: AnimationSpec::default().with_spring(SpringPreset::Default),
            exit: Motion::ExpandWidth(px(340.)),
            exit_spec: AnimationSpec::default(),
        }
    }

    /// tooltip / 小浮层：快速淡入淡出，不打扰内容阅读。
    ///
    /// 入场与退场均为 `Fade` + `fast`（120ms EaseOut）。
    pub fn tooltip() -> Self {
        Self {
            enter: Motion::Fade,
            enter_spec: AnimationSpec::fast(),
            exit: Motion::Fade,
            exit_spec: AnimationSpec::fast(),
        }
    }

    /// 弹窗：淡入（Spring 温和过冲营造轻柔落地感），退场慢速淡出。
    ///
    /// 入场 `Fade` + `default` + `Spring::Gentle`；退场 `Fade` + `slow`（350ms EaseInOut）。
    pub fn modal() -> Self {
        Self {
            enter: Motion::Fade,
            enter_spec: AnimationSpec::default().with_spring(SpringPreset::Gentle),
            exit: Motion::Fade,
            exit_spec: AnimationSpec::slow(),
        }
    }

    /// 通知条（右侧滑入）：入场略慢以聚焦内容，退场快速收回。
    ///
    /// 入场 `SlideLeft(40px)` + `fast`（250ms）；退场 `SlideLeft(40px)` + `fast`。
    pub fn notification() -> Self {
        Self {
            enter: Motion::SlideLeft(px(40.)),
            enter_spec: AnimationSpec::fast().with_duration(Duration::from_millis(250)),
            exit: Motion::SlideLeft(px(40.)),
            exit_spec: AnimationSpec::fast(),
        }
    }

    /// 下拉 / 折叠：垂直高度展开收起。
    ///
    /// 入场与退场均为 `ExpandHeight(200px)` + `default`（200ms EaseOut）。
    pub fn dropdown() -> Self {
        Self {
            enter: Motion::ExpandHeight(px(200.)),
            enter_spec: AnimationSpec::default(),
            exit: Motion::ExpandHeight(px(200.)),
            exit_spec: AnimationSpec::default(),
        }
    }

    /// 轻提示（底部滑入）：入场 Spring 轻微过冲，退场快速滑出。
    ///
    /// 入场 `SlideUp(16px)` + `default` + `Spring::Default`；退场 `SlideUp(16px)` + `fast`。
    pub fn toast() -> Self {
        Self {
            enter: Motion::SlideUp(px(16.)),
            enter_spec: AnimationSpec::default().with_spring(SpringPreset::Default),
            exit: Motion::SlideUp(px(16.)),
            exit_spec: AnimationSpec::fast(),
        }
    }

    /// 全自定义配对。
    ///
    /// `exit_spec` 经 [`AnimationSpec::without_spring`] 强制剥离 Spring（I3 防线）；
    /// 若其时长等于某 Spring 预设的推荐时长且未被显式覆盖，则重置为默认 200ms。
    pub fn custom(
        enter: Motion,
        enter_spec: AnimationSpec,
        exit: Motion,
        exit_spec: AnimationSpec,
    ) -> Self {
        Self {
            enter,
            enter_spec,
            exit,
            exit_spec: exit_spec.without_spring(),
        }
    }

    /// 转成入场 / 退场生命周期配对（[`MotionLifecycle`]）。
    ///
    /// 直接按字段构造——`exit_spec` 在本类型内恒为无 Spring（I3），无需再次剥离。
    pub fn lifecycle(self) -> MotionLifecycle {
        MotionLifecycle {
            enter: self.enter,
            exit: self.exit,
            enter_spec: self.enter_spec,
            exit_spec: self.exit_spec,
        }
    }

    /// 入场包装：将任意 `IntoElement + Styled` 元素与入场动效绑定，
    /// 返回 [`Animated<T>`]，可直接插入元素树（渲染期自动播放入场动画）。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use gpui_component_motion::MotionTokens;
    /// use gpui::{div, Styled};
    ///
    /// let el = MotionTokens::panel().animate(div().child("content"), "panel");
    /// ```
    pub fn animate<T>(self, inner: T, id: impl Into<ElementId>) -> Animated<T>
    where
        T: IntoElement + Styled + 'static,
    {
        Animated::new(inner, id, self.enter_spec, self.enter)
    }

    /// 覆盖入场动效与规格（退场保持不变）。
    pub fn with_enter(mut self, motion: Motion, spec: AnimationSpec) -> Self {
        self.enter = motion;
        self.enter_spec = spec;
        self
    }

    /// 覆盖退场动效与规格（入场保持不变）。
    ///
    /// 传入的规格经 [`AnimationSpec::without_spring`] 强制剥离 Spring（I3 防线）。
    pub fn with_exit(mut self, motion: Motion, spec: AnimationSpec) -> Self {
        self.exit = motion;
        self.exit_spec = spec.without_spring();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::div;

    /// 每个预设的入场 / 退场动效与规格断言（时长 / 缓动 / Spring 有无）。
    #[test]
    fn panel_preset() {
        let t = MotionTokens::panel();
        assert_eq!(t.enter, Motion::ExpandWidth(px(340.)));
        assert_eq!(t.enter_spec.spring, Some(SpringPreset::Default));
        assert_eq!(
            t.enter_spec.duration,
            SpringPreset::Default.recommended_duration()
        );
        assert_eq!(t.exit, Motion::ExpandWidth(px(340.)));
        assert_eq!(t.exit_spec, AnimationSpec::default());
    }

    #[test]
    fn tooltip_preset() {
        let t = MotionTokens::tooltip();
        assert_eq!(t.enter, Motion::Fade);
        assert_eq!(t.exit, Motion::Fade);
        assert_eq!(t.enter_spec, AnimationSpec::fast());
        assert_eq!(t.exit_spec, AnimationSpec::fast());
    }

    #[test]
    fn modal_preset() {
        let t = MotionTokens::modal();
        assert_eq!(t.enter, Motion::Fade);
        assert_eq!(t.enter_spec.spring, Some(SpringPreset::Gentle));
        assert_eq!(t.exit, Motion::Fade);
        assert_eq!(t.exit_spec, AnimationSpec::slow());
    }

    #[test]
    fn notification_preset() {
        let t = MotionTokens::notification();
        assert_eq!(t.enter, Motion::SlideLeft(px(40.)));
        assert_eq!(t.enter_spec.duration, Duration::from_millis(250));
        assert_eq!(t.enter_spec.easing, crate::Easing::EaseOut);
        assert_eq!(t.enter_spec.spring, None);
        assert_eq!(t.exit, Motion::SlideLeft(px(40.)));
        assert_eq!(t.exit_spec, AnimationSpec::fast());
    }

    #[test]
    fn dropdown_preset() {
        let t = MotionTokens::dropdown();
        assert_eq!(t.enter, Motion::ExpandHeight(px(200.)));
        assert_eq!(t.exit, Motion::ExpandHeight(px(200.)));
        assert_eq!(t.enter_spec, AnimationSpec::default());
        assert_eq!(t.exit_spec, AnimationSpec::default());
    }

    #[test]
    fn toast_preset() {
        let t = MotionTokens::toast();
        assert_eq!(t.enter, Motion::SlideUp(px(16.)));
        assert_eq!(t.enter_spec.spring, Some(SpringPreset::Default));
        assert_eq!(t.exit, Motion::SlideUp(px(16.)));
        assert_eq!(t.exit_spec, AnimationSpec::fast());
    }

    /// I3：所有预设的退场规格都不携带 Spring（遍历断言）。
    #[test]
    fn all_presets_exit_specs_have_no_spring() {
        for t in [
            MotionTokens::panel(),
            MotionTokens::tooltip(),
            MotionTokens::modal(),
            MotionTokens::notification(),
            MotionTokens::dropdown(),
            MotionTokens::toast(),
        ] {
            assert_eq!(
                t.exit_spec.spring, None,
                "预设 {:?} 的退场规格不应携带 Spring",
                t.enter
            );
        }
    }

    /// `custom` 传入含 Spring 的退场规格被剥离；未显式覆盖时长时重置为默认 200ms。
    #[test]
    fn custom_strips_spring_from_exit_spec() {
        let t = MotionTokens::custom(
            Motion::Fade,
            AnimationSpec::fast(),
            Motion::Fade,
            AnimationSpec::default().with_spring(SpringPreset::Wobbly),
        );
        assert_eq!(t.exit_spec.spring, None);
        assert_eq!(t.exit_spec.duration, AnimationSpec::default().duration);
        assert_eq!(t.enter_spec, AnimationSpec::fast());

        // 显式覆盖时长 → 时长保留，仅剥离 Spring
        let t = MotionTokens::custom(
            Motion::Fade,
            AnimationSpec::fast(),
            Motion::Fade,
            AnimationSpec::default()
                .with_spring(SpringPreset::Wobbly)
                .with_duration(Duration::from_millis(600)),
        );
        assert_eq!(t.exit_spec.spring, None);
        assert_eq!(t.exit_spec.duration, Duration::from_millis(600));
    }

    /// `with_exit` 传入含 Spring 的退场规格被剥离；入场侧不受影响。
    #[test]
    fn with_exit_strips_spring() {
        let t = MotionTokens::panel().with_exit(
            Motion::Fade,
            AnimationSpec::default().with_spring(SpringPreset::Default),
        );
        assert_eq!(t.exit, Motion::Fade);
        assert_eq!(t.exit_spec.spring, None);
        assert_eq!(t.exit_spec.duration, AnimationSpec::default().duration);
        // 入场侧保持不变
        assert_eq!(t.enter, Motion::ExpandWidth(px(340.)));
        assert_eq!(t.enter_spec.spring, Some(SpringPreset::Default));
    }

    /// `with_enter` 仅覆盖入场侧，退场侧不受影响。
    #[test]
    fn with_enter_overrides_enter_only() {
        let t = MotionTokens::tooltip().with_enter(Motion::SlideUp(px(8.)), AnimationSpec::slow());
        assert_eq!(t.enter, Motion::SlideUp(px(8.)));
        assert_eq!(t.enter_spec, AnimationSpec::slow());
        assert_eq!(t.exit, Motion::Fade);
        assert_eq!(t.exit_spec, AnimationSpec::fast());
    }

    /// `lifecycle` 产出与令牌字段一致的 [`MotionLifecycle`]。
    #[test]
    fn lifecycle_matches_token_fields() {
        for t in [
            MotionTokens::panel(),
            MotionTokens::tooltip(),
            MotionTokens::modal(),
            MotionTokens::notification(),
            MotionTokens::dropdown(),
            MotionTokens::toast(),
        ] {
            let lc = t.lifecycle();
            assert_eq!(lc.enter, t.enter);
            assert_eq!(lc.enter_spec, t.enter_spec);
            assert_eq!(lc.exit, t.exit);
            assert_eq!(lc.exit_spec, t.exit_spec);
        }
    }

    /// `animate` 返回 `Animated<Div>`，可编译并支持退场链式调用。
    #[test]
    fn animate_wraps_div() {
        let animated: Animated<gpui::Div> = MotionTokens::panel().animate(div(), "id");
        let _ = animated.exit();
    }

    /// `PartialEq`：同预设相等，改动任一字段后不等。
    #[test]
    fn partial_eq() {
        assert_eq!(MotionTokens::panel(), MotionTokens::panel());
        assert_eq!(MotionTokens::toast(), MotionTokens::toast());

        let base = MotionTokens::modal();
        assert_ne!(base, base.with_enter(Motion::Fade, AnimationSpec::fast()));
        assert_ne!(base, base.with_exit(Motion::Fade, AnimationSpec::fast()));
        assert_ne!(MotionTokens::panel(), MotionTokens::dropdown());
    }
}
