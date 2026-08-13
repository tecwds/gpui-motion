//! 为 [gpui-component](https://github.com/longbridge/gpui-component) 提供的非侵入式动画层。
//!
//! 在 GPUI 的 [`Animation`][gpui::Animation] 基座之上封装了预设动效（[`Motion`]）、
//! Spring 物理缓动（[`SpringPreset`]）、声明式生命周期容器（[`PresenceState`]）
//! 与元素扩展 trait（[`MotionExt`]），让组件的入场 / 退场动画只需一行链式调用即可接入，
//! 无需手动管理 `Animation` 状态。
//!
//! # 快速上手
//!
//! ## 入场动画
//!
//! 通过 [`MotionExt`] 为任意 `IntoElement + Styled` 元素添加入场动效：
//!
//! ```ignore
//! use gpui_component_motion::MotionExt;
//! use gpui::{div, px, Styled};
//!
//! div()
//!     .child(div().fade_in("my-fade"))
//!     .child(div().slide_up("my-slide", px(10.0)));
//! ```
//!
//! ## Spring 物理缓动
//!
//! ```ignore
//! use gpui_component_motion::{AnimationSpec, SpringPreset, MotionExt};
//! use gpui::{div, Styled};
//!
//! div().fade_in("spring").with_spec(
//!     AnimationSpec::default().with_spring(SpringPreset::Default),
//! );
//! ```
//!
//! ## 声明式 Presence（入场 + 退场）
//!
//! ```ignore
//! use gpui_component_motion::{AnimationSpec, MotionLifecycle, PresenceState};
//! use gpui::{div, px, Styled};
//!
//! let presence = PresenceState::new(
//!     cx,
//!     "panel",
//!     MotionLifecycle::expand_width(px(340.), AnimationSpec::default()),
//!     move |_window, _cx| div().w(px(340.)).h_full().child("content"),
//! );
//! ```
//!
//! # 架构
//!
//! | 类型 | 职责 |
//! |------|------|
//! | [`Easing`] | 传统缓动曲线（Linear / EaseIn / EaseOut / EaseInOut），输出 ∈ [0, 1] |
//! | [`SpringPreset`] | Spring 物理缓动（Stiff / Default / Gentle / Wobbly），输出可 > 1（过冲） |
//! | [`AnimationSpec`] | 时长 + 延迟 + 缓动 + Spring 预设 |
//! | [`Motion`] | 预设动效（Fade / Slide* / ExpandWidth / ExpandHeight） |
//! | [`Animated<T>`] | 元素包装器，实现 `IntoElement` 与 `ParentElement`，预设起始态、Spring 映射 |
//! | [`MotionExt`] | blanket impl，为 `IntoElement + Styled` 提供链式快捷方法 |
//! | [`MotionLifecycle`] | 入场 / 退场动效配对 |
//! | [`PresenceState`] | 声明式生命周期容器（HIDDEN→ENTERING→VISIBLE→EXITING→HIDDEN） |
//!
//! # 行为约定与已知限制
//!
//! - **退场强制剥离 Spring**：退场规格经 [`AnimationSpec::without_spring`] 统一处理
//!   （`Animated::new` / `Animated::with_exit` / `MotionLifecycle` 构造路径全覆盖），
//!   保证退场曲线严格单调（I3）；若时长未被显式覆盖（等于 Spring 推荐时长）则重置为默认 200ms。
//! - **过渡期快照**：`PresenceState` 在状态转换瞬间捕获入场 / 退场动效与规格，
//!   进行中的动画不受后续 [`PresenceState::set_lifecycle`] 影响；相等 lifecycle 调用直接短路。
//! - **卸载宽限期**：退场动画结束后额外等待 50ms 再卸载元素，
//!   兜底 GPUI 动画起点在首次 layout 才打点的滞后（至多一帧）。
//! - **终态样式覆盖**：动画在 `t=1` 应用 Motion 终态样式，会覆盖元素上与该动效
//!   冲突的既有样式（GPUI `Styled` refinement 不可移除、不可读回，属框架限制）；
//!   建议将动画应用于包装元素。
//! - **打断跳变**：GPUI `Animation` 不支持自定义起始进度，退场→入场 / 入场→退场
//!   打断时存在一次可见跳变（S8，已知限制，无规避）。
//! - **Expand 重排成本**：`ExpandWidth` / `ExpandHeight` 每帧触发 taffy 子树重排与
//!   文本重排版，大文本子树慎用（S10）。
//! - **delay 重绘成本**：`delay` 折叠进缓动前缀，延迟期仍每帧重绘，长 delay（>300ms）
//!   需知悉成本（S11）。

#![warn(missing_docs)]

mod animated;
mod easing;
mod ext;
mod lifecycle;
mod motion;
mod presence;
mod spec;

pub use animated::Animated;
pub use easing::{Easing, SpringPreset};
pub use ext::MotionExt;
pub use lifecycle::MotionLifecycle;
pub use motion::Motion;
pub use presence::PresenceState;
pub use spec::AnimationSpec;
