//! Gallery：展示 gpui-component 主要组件，并为每个区块应用 MotionExt 入场动画。
//!
//! 运行：`just example gallery`
//!
//! 展示内容：
//! - Button / Badge / Tag / Avatar / Icon / Spinner
//! - Switch / Checkbox / Radio / Slider / Input
//! - Progress / Tooltip / Notification
//! - Motion Composition（Phase 1：颜色插值 / Keyframes / Stagger）
//! - Loop & SpringValue（Phase 2：循环动效 / 数值弹簧）
//! - PresenceSet & DragSpring（Phase 3：多子元素进出 / 手势拖拽弹簧）
//! - Motion Tokens（Phase 4：组件动效令牌 / 零配置入场预设）
//! - 每个区块容器使用 SlideUp 入场，卡片内部使用 FadeIn 交错入场
//! - 点击 Replay 重播全部动画；点击右上角按钮切换深/浅色主题

use std::time::Duration;

use gpui::{
    AnyElement, App, Bounds, Context, ElementId, InteractiveElement, IntoElement, MouseButton,
    Render, Styled, Window, WindowBounds, WindowOptions, div, hsla, prelude::*, px, size,
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
use gpui_component_motion::{
    AnimationSpec, DragSpring, Easing, LoopKind, LoopMotion, Motion, MotionExt, MotionKeyframes,
    MotionLifecycle, MotionTokens, PresenceSet, SpringPreset, SpringValue, stagger,
};
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
    /// Phase 2（C7）: 循环动效开关 —— 仅 true 时挂载 LoopMotion。
    /// 循环组件永不结束、每帧 tick，空闲（false）绝不挂载（E1/E2 教训）。
    loop_on: bool,
    /// Phase 2（D8）: 数值弹簧实体（Wobbly 预设，允许过冲）。
    spring: gpui::Entity<SpringValue>,
    /// D8: 弹簧 tick 的订阅句柄 —— 必须持有（Drop 即取消 observe），
    /// 否则弹簧每帧 notify 无法驱动本 view 重绘。
    _spring_sub: gpui::Subscription,
    /// Phase 3（E10）: 多子元素声明式进出容器 —— 按 key 管理多个 child 的进出场，
    /// 退场完成自动移除条目；每个 key 独立 epoch / 快照 / 50ms 宽限。
    presence_set: gpui::Entity<PresenceSet>,
    /// E10: 三个演示开关，对应 key "a" / "b" / "c" 的期望存在状态。
    item_a: bool,
    item_b: bool,
    item_c: bool,
    /// Phase 3（D9）: 手势驱动弹簧 —— 拖拽期追赶指针，松手回弹 settle 点。
    drag_spring: gpui::Entity<DragSpring>,
    /// D9: 按下时记录的指针起点（用于把窗口坐标换算为拖拽位移）。
    drag_start_x: f32,
    /// E10/D9: PresenceSet / DragSpring 内部 notify 的订阅句柄 ——
    /// 必须持有，否则无法驱动本 view 重绘。
    _ps_sub: gpui::Subscription,
    _drag_sub: gpui::Subscription,
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
        // Phase 2（D8）: 数值弹簧 —— 初始值 0，Wobbly 预设（明显振荡、可过冲）。
        let spring = SpringValue::new(cx, 0.0, SpringPreset::Wobbly);
        // 订阅弹簧每帧 notify，驱动本 view 重绘（否则显示值不会更新）。
        let _spring_sub = cx.observe(&spring, |_, _, cx| cx.notify());
        // Phase 3（E10）: 多子元素进出容器 —— fade 配对；builder 每帧按 key 重建色块。
        let presence_set = PresenceSet::new(
            cx,
            MotionLifecycle::fade(AnimationSpec::default()),
            |key, _window, cx| {
                // 色块内容静态（key 文本），无每帧分配；theme 每帧现取以跟随主题切换。
                div()
                    .w(px(140.))
                    .h(px(56.))
                    .rounded_lg()
                    .bg(cx.theme().accent)
                    .child(div().text_sm().child(key.clone()))
            },
        );
        // E10: set_present / 退场定时器都会 notify PresenceSet —— 订阅驱动本 view 重绘。
        let _ps_sub = cx.observe(&presence_set, |_, _, cx| cx.notify());
        // Phase 3（D9）: 手势驱动弹簧 —— 初始值 0，Default 预设（轻微阻尼跟手）。
        let drag_spring = DragSpring::new(cx, 0.0, SpringPreset::Default);
        // D9: 弹簧每帧 tick notify —— 订阅驱动本 view 重绘（否则卡片位置不更新）。
        let _drag_sub = cx.observe(&drag_spring, |_, _, cx| cx.notify());
        Self {
            replay_count: 0,
            dark: false,
            loading: false,
            loop_on: false,
            spring,
            _spring_sub,
            presence_set,
            item_a: false,
            item_b: false,
            item_c: false,
            drag_spring,
            drag_start_x: 0.0,
            _ps_sub,
            _drag_sub,
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

    /// Motion Composition（Phase 1）：颜色插值 / Keyframes / Stagger 组合动效演示。
    fn render_motion_composition(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let n = self.replay_count;

        // —— 1) 颜色插值（A1）：入场时背景色 / 文字色从 from 渐变到 to（色相走最短路径）——
        let color_spec = AnimationSpec::default().with_duration(Duration::from_millis(600));
        let red = hsla(0.0, 0.9, 0.5, 1.0);
        let blue = hsla(0.6, 0.9, 0.5, 1.0);
        let mut cards = vec![
            Self::card(
                cx,
                ElementId::named_usize("gallery-mc-color-bg", n),
                0,
                div().w(px(120.)).h(px(80.)).rounded_lg().with_motion(
                    ElementId::named_usize("gallery-mc-color-bg-anim", n),
                    color_spec,
                    Motion::BackgroundColor(red, blue),
                ),
            ),
            Self::card(
                cx,
                ElementId::named_usize("gallery-mc-color-text", n),
                60,
                div().text_xl().child("Text Color").with_motion(
                    ElementId::named_usize("gallery-mc-color-text-anim", n),
                    color_spec,
                    Motion::TextColor(red, blue),
                ),
            ),
        ];

        // —— 2) Keyframes（B4）：两段式动画，前 50% 淡入 + 后 50% 上滑 ——
        cards.push(Self::card(
            cx,
            ElementId::named_usize("gallery-mc-kf-card", n),
            0,
            MotionKeyframes::new(
                div()
                    .w(px(120.))
                    .h(px(80.))
                    .rounded_lg()
                    .bg(cx.theme().accent),
                ElementId::named_usize("gallery-mc-kf", n),
                Duration::from_millis(600),
            )
            .keyframe(1.0, Easing::EaseOut, Motion::Fade)
            .keyframe(1.0, Easing::EaseOut, Motion::SlideUp(px(16.0))),
        ));

        // —— 3) Stagger（B5）：4 个色块 fade_in 后统一级联，第 i 个延迟 80ms * i ——
        let staggered = stagger(
            vec![
                div()
                    .w(px(40.))
                    .h(px(40.))
                    .rounded_md()
                    .bg(cx.theme().accent)
                    .fade_in(ElementId::named_usize("gallery-mc-stagger-0", n)),
                div()
                    .w(px(40.))
                    .h(px(40.))
                    .rounded_md()
                    .bg(cx.theme().accent)
                    .fade_in(ElementId::named_usize("gallery-mc-stagger-1", n)),
                div()
                    .w(px(40.))
                    .h(px(40.))
                    .rounded_md()
                    .bg(cx.theme().accent)
                    .fade_in(ElementId::named_usize("gallery-mc-stagger-2", n)),
                div()
                    .w(px(40.))
                    .h(px(40.))
                    .rounded_md()
                    .bg(cx.theme().accent)
                    .fade_in(ElementId::named_usize("gallery-mc-stagger-3", n)),
            ],
            Duration::from_millis(80),
        );
        cards.push(Self::card(
            cx,
            ElementId::named_usize("gallery-mc-stagger-card", n),
            0,
            h_flex().gap_2().children(staggered),
        ));

        self.section_wrapper(cx, "Motion Composition", "gallery-mc-section", 8, cards)
    }

    /// Loop & SpringValue（Phase 2）：循环动效 + 数值弹簧演示。
    fn render_loop_spring(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let n = self.replay_count;

        // —— 1) Loop 循环动效（C7 / LoopMotion）——
        // E1/E2（Critical）: 循环组件永不结束、每帧 tick —— 空闲（loop_on == false）
        // 绝不挂载，仅需循环效果时挂载；切换关闭即卸载（卸载即停，无后台残留）。
        let loop_block: AnyElement = if self.loop_on {
            // LoopKind::Pulse：平滑呼吸，透明度 0.4 → 1.0 → 0.4（900ms 周期；
            // 等价于便捷构造 LoopMotion::pulse(div(), id, 900ms)）。
            LoopMotion::new(
                div()
                    .w(px(120.))
                    .h(px(80.))
                    .rounded_lg()
                    .bg(cx.theme().accent),
                ElementId::named_usize("gallery-ls-loop-anim", n),
                LoopKind::Pulse,
                Duration::from_millis(900),
            )
            .into_any_element()
        } else {
            // 空闲：渲染静态块，不挂载任何循环动画。
            div()
                .w(px(120.))
                .h(px(80.))
                .rounded_lg()
                .bg(cx.theme().accent)
                .into_any_element()
        };
        let loop_card = Self::card(
            cx,
            ElementId::named_usize("gallery-ls-loop", n),
            0,
            v_flex().gap_3().items_center().child(loop_block).child(
                Button::new("loop-toggle")
                    .ghost()
                    .label(if self.loop_on {
                        "Stop pulsing"
                    } else {
                        "Start pulsing"
                    })
                    .on_click(cx.listener(move |v, _event, _window, cx| {
                        // 循环组件永不结束：空闲不挂载，切换即挂载 / 卸载（卸载即停）。
                        v.loop_on = !v.loop_on;
                        cx.notify();
                    })),
            ),
        );

        // —— 2) SpringValue 数值弹簧（D8）——
        // 渲染期每帧读取当前值（弹簧 tick 每帧 notify，经 _spring_sub 订阅驱动重绘）。
        let spring_value = self.spring.read(cx).value();
        let spring_card = Self::card(
            cx,
            ElementId::named_usize("gallery-ls-spring", n),
            50,
            v_flex()
                .gap_3()
                .items_center()
                .child(
                    div()
                        .text_3xl()
                        .text_color(cx.theme().foreground)
                        // 单卡片值显示允许每帧 1 次 format!（E3 例外，demo 专用）。
                        .child(format!("{:.0}", spring_value)),
                )
                .child(
                    Button::new("spring-to-100")
                        .primary()
                        .label("Spring to 100")
                        .on_click(cx.listener(move |v, _event, window, cx| {
                            // D8: 声明目标值 —— Wobbly 预设从当前值起跳插值到 100（可过冲）。
                            v.spring.update(cx, |s, cx| s.set_target(100.0, window, cx));
                        })),
                ),
        );

        self.section_wrapper(
            cx,
            "Loop & SpringValue",
            "gallery-ls-section",
            9,
            vec![loop_card, spring_card],
        )
    }

    /// PresenceSet & DragSpring（Phase 3）：多子元素声明式进出 + 手势驱动弹簧演示。
    fn render_presence_drag(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let n = self.replay_count;
        let item_a = self.item_a;
        let item_b = self.item_b;
        let item_c = self.item_c;

        // —— 1) PresenceSet 多 key 进出（E10）——
        // 三个 key（"a"/"b"/"c"）彼此独立：各自入场 / 退场互不干扰，退场动画结束后
        // 条目自动从容器移除（对照 Phase 1 的 PresenceState 单 child 特例）。
        let a_btn = {
            let btn = Button::new("pd-a");
            let btn = if item_a { btn.primary() } else { btn.ghost() };
            btn.label("Item A")
                .on_click(cx.listener(move |v, _event, window, cx| {
                    // 切换期望状态并同步到容器；set_present 内部会 notify（经订阅驱动重绘）。
                    v.item_a = !v.item_a;
                    let present = v.item_a;
                    v.presence_set
                        .update(cx, |s, cx| s.set_present("a", present, window, cx));
                    cx.notify();
                }))
        };
        let b_btn = {
            let btn = Button::new("pd-b");
            let btn = if item_b { btn.primary() } else { btn.ghost() };
            btn.label("Item B")
                .on_click(cx.listener(move |v, _event, window, cx| {
                    v.item_b = !v.item_b;
                    let present = v.item_b;
                    v.presence_set
                        .update(cx, |s, cx| s.set_present("b", present, window, cx));
                    cx.notify();
                }))
        };
        let c_btn = {
            let btn = Button::new("pd-c");
            let btn = if item_c { btn.primary() } else { btn.ghost() };
            btn.label("Item C")
                .on_click(cx.listener(move |v, _event, window, cx| {
                    v.item_c = !v.item_c;
                    let present = v.item_c;
                    v.presence_set
                        .update(cx, |s, cx| s.set_present("c", present, window, cx));
                    cx.notify();
                }))
        };
        let presence_card = Self::card(
            cx,
            ElementId::named_usize("gallery-pd-presence", n),
            0,
            v_flex()
                .gap_3()
                .items_center()
                .child(h_flex().gap_2().child(a_btn).child(b_btn).child(c_btn))
                // PresenceSet 实体本身即元素：内部按 key 重建 child 并播入场 / 退场动画。
                .child(self.presence_set.clone()),
        );

        // —— 2) DragSpring 手势拖拽（D9）——
        // 卡片 left 由弹簧值驱动：按下记录起点并 begin_drag；移动中 drag_to 追赶指针；
        // 松手 end_drag(0.0) 回弹到轨道起点。弹簧每帧 tick notify，经 _drag_sub 驱动重绘。
        let drag_value = self.drag_spring.read(cx).value();
        let drag_card = Self::card(
            cx,
            ElementId::named_usize("gallery-pd-drag", n),
            50,
            v_flex().gap_3().items_center().child(
                div()
                    .relative()
                    .w(px(320.))
                    .h(px(80.))
                    .rounded_lg()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().accent.opacity(0.06))
                    .overflow_hidden()
                    .child(
                        div()
                            .absolute()
                            .top(px(8.))
                            .left(px(drag_value))
                            .w(px(64.))
                            .h(px(64.))
                            .rounded_md()
                            .bg(cx.theme().accent)
                            .cursor_pointer()
                            // E3: 交互元素同样用稳定基名 + replay_count（render 内零 format! 造 id）。
                            .id(ElementId::named_usize("gallery-pd-drag-card", n))
                            // 事件接线（回调签名已按 gpui 实际 API 核实）：
                            // on_mouse_down / on_mouse_up 需指定按键；三个回调均为
                            // Fn(&Mouse*Event, &mut Window, &mut App)，经 cx.listener 取视图状态。
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |v, event: &gpui::MouseDownEvent, window, cx| {
                                    // 按下：记录指针起点，进入跟手模式（弹簧停在当前值）。
                                    v.drag_start_x = f32::from(event.position.x);
                                    v.drag_spring.update(cx, |s, cx| s.begin_drag(window, cx));
                                    cx.notify();
                                }),
                            )
                            .on_mouse_move(cx.listener(
                                move |v, event: &gpui::MouseMoveEvent, window, cx| {
                                    // 拖动：目标 = 指针位移（窗口坐标 - 按下起点），弹簧阻尼追赶。
                                    let dx = f32::from(event.position.x) - v.drag_start_x;
                                    v.drag_spring.update(cx, |s, cx| s.drag_to(dx, window, cx));
                                },
                            ))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(move |v, _event: &gpui::MouseUpEvent, window, cx| {
                                    // 松手：退出跟手模式，弹簧从当前值回弹并精确收敛到 0。
                                    v.drag_spring
                                        .update(cx, |s, cx| s.end_drag(0.0, window, cx));
                                }),
                            ),
                    ),
            ),
        );

        self.section_wrapper(
            cx,
            "PresenceSet & Drag",
            "gallery-pd-section",
            10,
            vec![presence_card, drag_card],
        )
    }

    /// Motion Tokens（Phase 4，F12）：常见组件的动效令牌 —— 零配置接入。
    ///
    /// 两种消费方式：`MotionTokens::xxx().animate(el, id)` 一行包装入场（渲染期自动播放，
    /// 无需记忆 Motion / AnimationSpec 配对）；`MotionTokens::xxx().lifecycle()` 产出
    /// MotionLifecycle 供 PresenceState / PresenceSet 使用（完整进出场配对）。
    /// 本演示展示 4 个预设的 animate 对比 —— Replay 换 id 即重播（E3/E4）。
    fn render_motion_tokens(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let n = self.replay_count;

        // —— 1) 一行入场：tooltip().animate(div(), id) 直接插入元素树，渲染期自动播放入场 ——
        let one_liner_card = Self::card(
            cx,
            ElementId::named_usize("gallery-tk-oneliner", n),
            0,
            v_flex()
                .gap_3()
                .items_center()
                .child(
                    // 零配置：Fade + fast（120ms EaseOut），无需自己拼 Motion / AnimationSpec。
                    MotionTokens::tooltip().animate(
                        div()
                            .w(px(120.))
                            .h(px(80.))
                            .rounded_lg()
                            .bg(cx.theme().accent),
                        ElementId::named_usize("gallery-tk-tooltip", n),
                    ),
                )
                .child(div().text_sm().child("tooltip().animate() 一行入场")),
        );

        // —— 2) 预设对比：tooltip / notification / toast / dropdown 各 animate 一个色块 ——
        // token 名与 id 基名都是 &'static str，直接作 child / id 基名（render 内零 format!）。
        let tokens: [(&'static str, MotionTokens, &'static str); 4] = [
            ("tooltip", MotionTokens::tooltip(), "gallery-tk-tooltip"),
            (
                "notification",
                MotionTokens::notification(),
                "gallery-tk-notification",
            ),
            ("toast", MotionTokens::toast(), "gallery-tk-toast"),
            ("dropdown", MotionTokens::dropdown(), "gallery-tk-dropdown"),
        ];
        let blocks: Vec<_> = tokens
            .iter()
            .map(|&(name, token, id_base)| {
                v_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        // E3: 稳定基名 + replay_count，避免 render 内 format! 造 id。
                        token.animate(
                            div()
                                .w(px(120.))
                                .h(px(80.))
                                .rounded_lg()
                                .bg(cx.theme().accent),
                            ElementId::named_usize(id_base, n),
                        ),
                    )
                    .child(div().text_sm().child(name))
            })
            .collect();
        let compare_card = Self::card(
            cx,
            ElementId::named_usize("gallery-tk-compare", n),
            60,
            v_flex()
                .gap_3()
                .items_center()
                .child(h_flex().gap_4().flex_wrap().children(blocks))
                // lifecycle() 消费方式说明（Phase 3 的 PresenceState / PresenceSet 用法）。
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("同款令牌亦可 .lifecycle() 供 PresenceState / PresenceSet 消费"),
                ),
        );

        self.section_wrapper(
            cx,
            "Motion Tokens",
            "gallery-tk-section",
            11,
            vec![one_liner_card, compare_card],
        )
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
            .child(self.render_tooltip_notification(cx))
            .child(self.render_motion_composition(cx))
            .child(self.render_loop_spring(cx))
            .child(self.render_presence_drag(cx))
            .child(self.render_motion_tokens(cx));

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
