//! Story：展示 gpui-component-motion 的全部动画效果。
//!
//! 运行：`just example story`
//!
//! 展示内容：
//! - 5 种 Motion：Fade / SlideUp / SlideDown / SlideLeft / SlideRight
//! - 4 种 Easing：Linear / EaseIn / EaseOut / EaseInOut
//! - 点击 Replay 重播所有动画
//!
//! 低成本范本（E4）：本示例是三个 demo 中每帧成本最低的 —— 无常驻
//! `repeat()` 组件、文本全部用 `&'static str` 零分配渲染、id 仅在 9 张
//! 静态卡片上各做一次 `format!` 拼接（无每帧文本分配）。Replay 采用
//! "换全部 ElementId"的 demo 惯用法（见下方 `replay_count`）；
//! 真实应用应保持稳定 id，只对变更的元素 re-notify。

use gpui::{
    App, Bounds, Context, InteractiveElement, IntoElement, MouseButton, Render, SharedString,
    Styled, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use gpui_platform::application;

use gpui_component_motion::{AnimationSpec, Easing, Motion, MotionExt};

/// 卡片信息：标题 + 对应的动画
struct MotionCard {
    title: &'static str,
    motion: Motion,
}

/// 缓动卡片信息：标题 + 对应的缓动曲线
struct EasingCard {
    title: &'static str,
    easing: Easing,
}

struct Story {
    /// 每次点击 Replay 时递增，用于生成新的 ElementId 以重启动画。
    ///
    /// 注意（E4）：demo 专属惯用法 —— 真实应用应保持稳定 id，
    /// 只对变更的元素 re-notify，而不是全局换 id 重挂所有动画。
    replay_count: usize,
}

impl Story {
    fn new() -> Self {
        Self { replay_count: 0 }
    }

    /// 动画卡片列表
    fn motion_cards() -> Vec<MotionCard> {
        vec![
            MotionCard {
                title: "Fade",
                motion: Motion::Fade,
            },
            MotionCard {
                title: "SlideUp",
                motion: Motion::SlideUp(px(24.0)),
            },
            MotionCard {
                title: "SlideDown",
                motion: Motion::SlideDown(px(24.0)),
            },
            MotionCard {
                title: "SlideLeft",
                motion: Motion::SlideLeft(px(24.0)),
            },
            MotionCard {
                title: "SlideRight",
                motion: Motion::SlideRight(px(24.0)),
            },
        ]
    }

    /// 缓动卡片列表（统一使用 SlideUp 以突出缓动差异）
    fn easing_cards() -> Vec<EasingCard> {
        vec![
            EasingCard {
                title: "Linear",
                easing: Easing::Linear,
            },
            EasingCard {
                title: "EaseIn",
                easing: Easing::EaseIn,
            },
            EasingCard {
                title: "EaseOut",
                easing: Easing::EaseOut,
            },
            EasingCard {
                title: "EaseInOut",
                easing: Easing::EaseInOut,
            },
        ]
    }

    /// 渲染单个 Motion 卡片
    fn render_motion_card(card: &MotionCard, id_prefix: &str) -> impl IntoElement {
        let id = format!("{id_prefix}-motion-{}", card.title);
        let bg = rgb(0x6366f1);
        let title: SharedString = card.title.into();

        div()
            .id(id.clone())
            .size(px(140.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_lg()
            .bg(bg)
            .text_color(rgb(0xffffff))
            .text_sm()
            .child(title)
            .with_motion(id, AnimationSpec::default(), card.motion)
    }

    /// 渲染单个 Easing 卡片
    fn render_easing_card(card: &EasingCard, id_prefix: &str) -> impl IntoElement {
        let id = format!("{id_prefix}-easing-{}", card.title);
        let bg = rgb(0xec4899);
        let title: SharedString = card.title.into();
        let spec = AnimationSpec::default().with_easing(card.easing);

        div()
            .id(id.clone())
            .size(px(140.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_lg()
            .bg(bg)
            .text_color(rgb(0xffffff))
            .text_sm()
            .child(title)
            .with_motion(id, spec, Motion::SlideUp(px(24.0)))
    }
}

impl Render for Story {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let id_prefix = format!("story-{}", self.replay_count);

        // 标题栏
        let header = div()
            .flex()
            .justify_between()
            .items_center()
            .child(
                div()
                    .text_xl()
                    .text_color(rgb(0xe2e8f0))
                    .child("gpui-component-motion Story"),
            )
            .child(
                div()
                    .id("replay-btn")
                    .cursor_pointer()
                    .px_4()
                    .py_2()
                    .rounded_md()
                    .bg(rgb(0x6366f1))
                    .text_color(rgb(0xffffff))
                    .text_sm()
                    .hover(|s| s.bg(rgb(0x818cf8)))
                    .child("Replay")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _, _, cx| {
                            view.replay_count += 1;
                            cx.notify();
                        }),
                    ),
            );

        // Motion 区块
        let motion_section = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_color(rgb(0x94a3b8)).text_sm().child("Motion"))
            .child(
                div().flex().flex_wrap().gap_4().children(
                    Self::motion_cards()
                        .iter()
                        .map(|c| Self::render_motion_card(c, &id_prefix)),
                ),
            );

        // Easing 区块
        let easing_section = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_color(rgb(0x94a3b8))
                    .text_sm()
                    .child("Easing (SlideUp 24px)"),
            )
            .child(
                div().flex().flex_wrap().gap_4().children(
                    Self::easing_cards()
                        .iter()
                        .map(|c| Self::render_easing_card(c, &id_prefix)),
                ),
            );

        // 主容器
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x0f172a))
            .gap_6()
            .p_6()
            .child(header)
            .child(motion_section)
            .child(easing_section)
    }
}

fn run_example() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(720.), px(480.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_window, cx| cx.new(|_| Story::new()),
        )
        .unwrap();
        cx.activate(true);
    });
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    run_example();
}
