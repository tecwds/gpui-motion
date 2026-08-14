//! Gallery：展示 gpui-component 主要组件，并为每个区块应用 MotionExt 入场动画。
//!
//! 运行：`just example gallery`
//!
//! 展示内容：
//! - Button / Badge / Tag / Avatar / Icon / Spinner
//! - Switch / Checkbox / Radio / Slider / Input
//! - Progress / Tooltip / Notification
//! - 每个区块容器使用 SlideUp 入场，卡片内部使用 FadeIn 交错入场
//! - 点击 Replay 重播全部动画；点击右上角按钮切换深/浅色主题

use std::time::Duration;

use gpui::{
    AnyElement, App, Bounds, Context, ElementId, InteractiveElement, IntoElement, Render, Styled,
    Window, WindowBounds, WindowOptions, div, prelude::*, px, size,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Root, Sizable as _, Theme, ThemeMode, WindowExt as _,
    avatar::Avatar,
    badge::Badge,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    notification::NotificationType,
    progress::{Progress, ProgressCircle},
    radio::Radio,
    slider::{Slider, SliderState},
    spinner::Spinner,
    switch::Switch,
    tag::Tag,
    v_flex,
};
use gpui_component_assets::Assets;
use gpui_component_motion::{AnimationSpec, MotionExt};
use gpui_platform::application;

struct Gallery {
    /// 每次点击 Replay 时递增，用于生成新的 ElementId 以重启动画。
    ///
    /// 注意（E4）：这是 demo 专属惯用法 —— 真实应用应保持稳定 id，
    /// 只对变更的元素 re-notify，而不是全局换 id 重挂所有动画。
    replay_count: usize,
    /// 是否深色主题
    dark: bool,
    /// 是否处于"加载中"状态：为 true 时才挂载 Spinner / loading ProgressCircle。
    /// 这些组件内部是 `Animation::repeat()`（永不 done，C10），常驻挂载会把窗口
    /// 钉在满帧率重绘整个会话；空闲时必须卸载（E1）。
    loading: bool,
    // 交互状态
    switch_val: bool,
    checkbox_val: bool,
    radio_val: usize,
    progress_val: f32,
    // 组件状态
    input: gpui::Entity<InputState>,
    slider: gpui::Entity<SliderState>,
}

impl Gallery {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Type something..."));
        let slider = cx.new(|_| SliderState::new().min(0.).max(100.).default_value(60.));
        Self {
            replay_count: 0,
            dark: false,
            loading: false,
            switch_val: true,
            checkbox_val: false,
            radio_val: 0,
            progress_val: 65.,
            input,
            slider,
        }
    }

    /// 构建一个带圆角边框的卡片容器，内部 content 居中。
    /// 卡片整体使用 FadeIn 动画，delay 由 `delay_ms` 控制以实现交错入场。
    fn card(
        cx: &Context<Self>,
        id: impl Into<gpui::ElementId>,
        delay_ms: u64,
        content: impl IntoElement,
    ) -> AnyElement {
        let spec = AnimationSpec::default()
            .with_delay(Duration::from_millis(delay_ms))
            .with_duration(Duration::from_millis(350));
        h_flex()
            .items_center()
            .justify_center()
            .p_4()
            .min_w(px(160.))
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().accent.opacity(0.08))
            .child(content)
            .fade_in(id)
            .with_spec(spec)
            .into_any_element()
    }

    /// section 标题
    fn section_title(cx: &Context<Self>, text: &'static str) -> impl IntoElement {
        div()
            .text_color(cx.theme().muted_foreground)
            .text_sm()
            .child(text)
    }

    // —— 各区块渲染 ——

    fn render_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // E3: 稳定基名 + replay_count 计数（NamedInteger），避免每帧 format! 造 id。
        let n = self.replay_count;
        let cards = vec![
            Self::card(
                cx,
                ElementId::named_usize("gallery-btn-default", n),
                0,
                Button::new("b1").label("Default"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-btn-primary", n),
                40,
                Button::new("b2").primary().label("Primary"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-btn-secondary", n),
                80,
                Button::new("b3").secondary().label("Secondary"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-btn-danger", n),
                120,
                Button::new("b4").danger().label("Danger"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-btn-success", n),
                160,
                Button::new("b5").success().label("Success"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-btn-warning", n),
                200,
                Button::new("b6").warning().label("Warning"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-btn-outline", n),
                240,
                Button::new("b7").outline().label("Outline"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-btn-ghost", n),
                280,
                Button::new("b8").ghost().label("Ghost"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-btn-link", n),
                320,
                Button::new("b9").link().label("Link"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-btn-icon", n),
                360,
                Button::new("b10")
                    .primary()
                    .icon(IconName::Check)
                    .label("Confirm"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-btn-icon-only", n),
                400,
                Button::new("b11").ghost().icon(IconName::Search),
            ),
        ];

        self.section_wrapper(cx, "Button", "gallery-btn-section", 0, cards)
    }

    fn render_badge_tag(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let n = self.replay_count;
        let cards = vec![
            Self::card(
                cx,
                ElementId::named_usize("gallery-bt-badge-1", n),
                0,
                Badge::new()
                    .count(3)
                    .child(Icon::new(IconName::Bell).large()),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-bt-badge-2", n),
                40,
                Badge::new()
                    .count(103)
                    .child(Icon::new(IconName::Inbox).large()),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-bt-tag-primary", n),
                80,
                Tag::primary().child("Tag"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-bt-tag-secondary", n),
                120,
                Tag::secondary().child("Secondary"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-bt-tag-danger", n),
                160,
                Tag::danger().child("Danger"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-bt-tag-success", n),
                200,
                Tag::success().child("Success"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-bt-tag-warning", n),
                240,
                Tag::warning().child("Warning"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-bt-tag-info", n),
                280,
                Tag::info().child("Info"),
            ),
        ];

        self.section_wrapper(cx, "Badge & Tag", "gallery-bt-section", 1, cards)
    }

    fn render_avatar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let n = self.replay_count;
        let cards = vec![
            Self::card(
                cx,
                ElementId::named_usize("gallery-av-img-1", n),
                0,
                Avatar::new().src("https://avatars.githubusercontent.com/u/5518?v=4"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-av-img-2", n),
                50,
                Avatar::new()
                    .large()
                    .src("https://avatars.githubusercontent.com/u/28998859?v=4"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-av-text-1", n),
                100,
                Avatar::new().large().name("Jason Lee"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-av-text-2", n),
                150,
                Avatar::new().name("Floyd Wang"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-av-placeholder", n),
                200,
                Avatar::new().small().placeholder(IconName::Building2),
            ),
        ];

        self.section_wrapper(cx, "Avatar", "gallery-av-section", 2, cards)
    }

    fn render_form_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let n = self.replay_count;
        let switch_val = self.switch_val;
        let checkbox_val = self.checkbox_val;
        let radio_val = self.radio_val;

        let cards = vec![
            Self::card(
                cx,
                ElementId::named_usize("gallery-fc-switch", n),
                0,
                Switch::new("sw1")
                    .checked(switch_val)
                    .on_click(cx.listener(move |v, c, _, cx| {
                        v.switch_val = *c;
                        cx.notify();
                    })),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-fc-checkbox", n),
                50,
                Checkbox::new("cb1")
                    .label("Accept")
                    .checked(checkbox_val)
                    .on_click(cx.listener(move |v, _, _, cx| {
                        v.checkbox_val = !v.checkbox_val;
                        cx.notify();
                    })),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-fc-radio-1", n),
                100,
                Radio::new("r1")
                    .label("Option A")
                    .checked(radio_val == 0)
                    .on_click(cx.listener(move |v, _, _, cx| {
                        v.radio_val = 0;
                        cx.notify();
                    })),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-fc-radio-2", n),
                150,
                Radio::new("r2")
                    .label("Option B")
                    .checked(radio_val == 1)
                    .on_click(cx.listener(move |v, _, _, cx| {
                        v.radio_val = 1;
                        cx.notify();
                    })),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-fc-radio-3", n),
                200,
                Radio::new("r3")
                    .label("Option C")
                    .checked(radio_val == 2)
                    .on_click(cx.listener(move |v, _, _, cx| {
                        v.radio_val = 2;
                        cx.notify();
                    })),
            ),
        ];

        self.section_wrapper(
            cx,
            "Switch / Checkbox / Radio",
            "gallery-fc-section",
            3,
            cards,
        )
    }

    fn render_slider(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let cards = vec![Self::card(
            cx,
            ElementId::named_usize("gallery-sl-slider", self.replay_count),
            0,
            Slider::new(&self.slider).w(px(200.)),
        )];

        self.section_wrapper(cx, "Slider", "gallery-sl-section", 4, cards)
    }

    fn render_input(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let cards = vec![Self::card(
            cx,
            ElementId::named_usize("gallery-in-input", self.replay_count),
            0,
            Input::new(&self.input).w(px(220.)),
        )];

        self.section_wrapper(cx, "Input", "gallery-in-section", 5, cards)
    }

    fn render_progress_spinner(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let n = self.replay_count;
        let progress_val = self.progress_val;
        let mut cards = vec![
            Self::card(
                cx,
                ElementId::named_usize("gallery-ps-progress", n),
                0,
                Progress::new("pg1").value(progress_val).w(px(200.)),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-ps-circle", n),
                50,
                ProgressCircle::new("pg2").value(progress_val).size_12(),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-ps-loading-toggle", n),
                75,
                Button::new("loading-toggle")
                    .ghost()
                    .label(if self.loading {
                        "Stop loading"
                    } else {
                        "Simulate loading"
                    })
                    .on_click(cx.listener(move |v, _event, window, cx| {
                        v.loading = !v.loading;
                        if v.loading {
                            // E1: 2.5s 后自动回到空闲并卸载 repeat() 组件，
                            // 避免演示结束后仍满帧重绘。
                            cx.spawn_in(window, async move |this, cx| {
                                cx.background_executor()
                                    .timer(Duration::from_millis(2500))
                                    .await;
                                _ = this.update_in(cx, |v, _window, cx| {
                                    v.loading = false;
                                    cx.notify();
                                });
                            })
                            .detach();
                        }
                        cx.notify();
                    })),
            ),
        ];

        // E1（Critical）: Spinner / loading ProgressCircle 内部是 `Animation::repeat()`
        // （永不 done，C10）—— 常驻挂载会把窗口钉在满帧率重绘整个会话。
        // 仅当 `loading == true` 时挂载；空闲（idle）时绝不挂载这些组件。
        if self.loading {
            cards.push(Self::card(
                cx,
                ElementId::named_usize("gallery-ps-circle-loading", n),
                100,
                ProgressCircle::new("pg3").loading(true).size_12(),
            ));
            cards.push(Self::card(
                cx,
                ElementId::named_usize("gallery-ps-spinner", n),
                150,
                Spinner::new(),
            ));
            cards.push(Self::card(
                cx,
                ElementId::named_usize("gallery-ps-spinner-lg", n),
                200,
                Spinner::new().large().color(cx.theme().blue),
            ));
            cards.push(Self::card(
                cx,
                ElementId::named_usize("gallery-ps-spinner-sm", n),
                250,
                Spinner::new().small().color(cx.theme().green),
            ));
        }

        self.section_wrapper(cx, "Progress & Spinner", "gallery-ps-section", 6, cards)
    }

    fn render_tooltip_notification(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let n = self.replay_count;
        let cards = vec![
            Self::card(
                cx,
                ElementId::named_usize("gallery-tn-tooltip", n),
                0,
                Button::new("tt1")
                    .label("Hover me")
                    .tooltip("This is a tooltip"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-tn-tooltip-icon", n),
                50,
                Button::new("tt2")
                    .ghost()
                    .icon(IconName::Info)
                    .tooltip("Icon button tooltip"),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-tn-notify", n),
                100,
                Button::new("nt1")
                    .primary()
                    .label("Notify")
                    .on_click(|_, window, cx| {
                        window.push_notification("Hello from gallery!", cx);
                    }),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-tn-notify-info", n),
                100,
                Button::new("nt2")
                    .info()
                    .label("Info")
                    .on_click(|_, window, cx| {
                        window.push_notification(
                            (NotificationType::Info, "This is an info notification."),
                            cx,
                        );
                    }),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-tn-notify-success", n),
                150,
                Button::new("nt3")
                    .success()
                    .label("Success")
                    .on_click(|_, window, cx| {
                        window.push_notification(
                            (
                                NotificationType::Success,
                                "Operation completed successfully.",
                            ),
                            cx,
                        );
                    }),
            ),
        ];

        self.section_wrapper(cx, "Tooltip & Notification", "gallery-tn-section", 7, cards)
    }

    /// section 容器：标题 + 卡片网格，整体使用 SlideUp 动画入场。
    fn section_wrapper(
        &self,
        cx: &Context<Self>,
        title: &'static str,
        id_base: &'static str,
        section_idx: usize,
        cards: Vec<AnyElement>,
    ) -> impl IntoElement {
        let spec = AnimationSpec::default()
            .with_delay(Duration::from_millis((section_idx * 80) as u64))
            .with_duration(Duration::from_millis(400));

        v_flex()
            .gap_3()
            .w_full()
            .child(Self::section_title(cx, title))
            .child(h_flex().flex_wrap().gap_3().children(cards))
            // E3: 稳定基名 + replay_count（NamedInteger），替代 format! 造 id。
            .slide_up(ElementId::named_usize(id_base, self.replay_count), px(24.0))
            .with_spec(spec)
    }
}

impl Render for Gallery {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = self.dark;

        // 顶部标题栏
        let header = {
            let replay_spec = AnimationSpec::default().with_duration(Duration::from_millis(300));
            h_flex()
                .justify_between()
                .items_center()
                .pb_4()
                .child(
                    v_flex()
                        .gap_1()
                        .child(div().text_xl().child("gpui-component Gallery"))
                        .child(
                            div()
                                .text_color(cx.theme().muted_foreground)
                                .text_sm()
                                .child("Each section animates in with MotionExt"),
                        ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("theme-toggle")
                                .ghost()
                                .icon(if dark { IconName::Sun } else { IconName::Moon })
                                .on_click(cx.listener(move |v, _, _, cx| {
                                    v.dark = !v.dark;
                                    Theme::change(
                                        if v.dark {
                                            ThemeMode::Dark
                                        } else {
                                            ThemeMode::Light
                                        },
                                        None,
                                        cx,
                                    );
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("replay")
                                .primary()
                                .icon(IconName::Play)
                                .label("Replay")
                                .on_click(cx.listener(move |v, _, _, cx| {
                                    // E4: demo 惯用法 —— 换全部 ElementId 重放动画。
                                    // 真实应用应保持稳定 id，只对变更的元素 re-notify，
                                    // 而不是全局换 id 重挂所有动画。
                                    v.replay_count += 1;
                                    cx.notify();
                                })),
                        )
                        .fade_in(ElementId::named_usize(
                            "gallery-header-btns",
                            self.replay_count,
                        ))
                        .with_spec(replay_spec),
                )
        };

        // 滚动内容区
        let content = v_flex()
            .gap_8()
            .child(self.render_buttons(cx))
            .child(self.render_badge_tag(cx))
            .child(self.render_avatar(cx))
            .child(self.render_form_controls(cx))
            .child(self.render_slider(cx))
            .child(self.render_input(cx))
            .child(self.render_progress_spinner(cx))
            .child(self.render_tooltip_notification(cx));

        let scroll_area = div()
            .id("gallery-scroll")
            .flex_1()
            .overflow_y_scroll()
            .p_6()
            .child(content);

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(header)
            .child(scroll_area)
    }
}

fn run_example() {
    application().with_assets(Assets).run(|cx: &mut App| {
        gpui_component::init(cx);
        cx.activate(true);

        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| Gallery::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .unwrap();
    });
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    run_example();
}
