//! Codex：复刻 OpenAI Codex 桌面版工作区布局。
//!
//! 运行：`just example codex`
//!
//! 布局参考：Codex Desktop App（bundle id `com.openai.codex`）
//! - 左侧边栏：New chat / Search / Plugins / Automations / Pinned / Projects / Chats / 账号 + Settings
//! - 中间对话区：顶部 chat 标题 + 模型 badge；消息流（user / assistant / 代码块 / 工具调用 / diff）；底部 composer
//! - 右侧面板：可开/关，含 Browser / Files / Terminal / Summary 标签
//!
//! 动画（MotionExt）：
//! - 侧边栏 slide_right 入场
//! - 顶部栏 fade_in；消息依次 slide_up 交错入场
//! - 右侧面板 slide_left 入场；composer slide_up 入场
//! - 点击 Replay 重播全部动画；右上角按钮切换深/浅主题、开/关右侧面板

use std::time::Duration;

use gpui::{
    App, Bounds, Context, InteractiveElement, IntoElement, Render, SharedString, Styled, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Root, Sizable as _, Theme, ThemeMode, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    v_flex,
};
use gpui_component_assets::Assets;
use gpui_component_motion::{
    AnimationSpec, MotionExt, MotionLifecycle, PresenceState, SpringPreset,
};
use gpui_platform::application;

/// OpenAI 品牌绿（Codex 强调色）
fn openai_green() -> gpui::Rgba {
    rgb(0x10a37f)
}

/// 右侧面板标签页
#[derive(Clone, Copy, PartialEq)]
enum RightPanelTab {
    Browser,
    Files,
    Terminal,
    Summary,
}

impl RightPanelTab {
    fn label(self) -> &'static str {
        match self {
            RightPanelTab::Browser => "Browser",
            RightPanelTab::Files => "Files",
            RightPanelTab::Terminal => "Terminal",
            RightPanelTab::Summary => "Summary",
        }
    }
    fn icon(self) -> IconName {
        match self {
            RightPanelTab::Browser => IconName::Globe,
            RightPanelTab::Files => IconName::Folder,
            RightPanelTab::Terminal => IconName::Frame,
            RightPanelTab::Summary => IconName::Inspector,
        }
    }
}

/// 侧边栏导航项
#[derive(Clone, Copy, PartialEq)]
enum NavItem {
    NewChat,
    Search,
    Plugins,
    Automations,
    Pinned,
}

impl NavItem {
    fn icon(self) -> IconName {
        match self {
            NavItem::NewChat => IconName::Plus,
            NavItem::Search => IconName::Search,
            NavItem::Plugins => IconName::LayoutDashboard,
            NavItem::Automations => IconName::Calendar,
            NavItem::Pinned => IconName::Star,
        }
    }
    fn label(self) -> &'static str {
        match self {
            NavItem::NewChat => "New chat",
            NavItem::Search => "Search",
            NavItem::Plugins => "Plugins",
            NavItem::Automations => "Automations",
            NavItem::Pinned => "Pinned",
        }
    }
}

struct ChatItem {
    title: &'static str,
    active: bool,
}

struct ProjectItem {
    name: &'static str,
    color: gpui::Rgba,
}

struct CodexApp {
    /// 每次点击 Replay 时递增，用于生成新的 ElementId 以重启动画
    replay_count: usize,
    dark: bool,
    right_panel_open: bool,
    /// 入场动画使用 Spring 物理缓动（true）或传统 EaseOut（false）。
    use_spring: bool,
    /// 右侧面板的声明式生命周期动画状态。
    right_panel_presence: gpui::Entity<PresenceState>,
    right_panel_tab: RightPanelTab,
    active_nav: NavItem,
    input: gpui::Entity<InputState>,
    chats: Vec<ChatItem>,
    projects: Vec<ProjectItem>,
}

impl CodexApp {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Ask Codex to build something..."));
        let weak = cx.entity().downgrade();
        let right_panel_presence = PresenceState::new(
            cx,
            "right-panel",
            MotionLifecycle::expand_width(
                px(340.),
                AnimationSpec::default().with_spring(SpringPreset::Default),
            )
            .with_exit_spec(AnimationSpec::default().with_duration(Duration::from_millis(250))),
            move |window, cx| {
                if let Some(strong) = weak.upgrade() {
                    strong
                        .read(cx)
                        .render_right_panel_inner(strong.clone(), window, cx)
                } else {
                    div()
                }
            },
        );
        Self {
            replay_count: 0,
            dark: true,
            right_panel_open: true,
            use_spring: true,
            right_panel_presence,
            right_panel_tab: RightPanelTab::Browser,
            active_nav: NavItem::NewChat,
            input,
            chats: vec![
                ChatItem {
                    title: "Refactor auth module",
                    active: true,
                },
                ChatItem {
                    title: "Add dark mode to landing page",
                    active: false,
                },
                ChatItem {
                    title: "Debug WebSocket reconnect",
                    active: false,
                },
                ChatItem {
                    title: "Migrate to Postgres 16",
                    active: false,
                },
                ChatItem {
                    title: "Set up CI with GitHub Actions",
                    active: false,
                },
            ],
            projects: vec![
                ProjectItem {
                    name: "wb-gpui",
                    color: openai_green(),
                },
                ProjectItem {
                    name: "marketing-site",
                    color: rgb(0x6366f1),
                },
                ProjectItem {
                    name: "api-server",
                    color: rgb(0xf59e0b),
                },
            ],
        }
    }

    // —— 侧边栏（使用 cx.listener，需 &mut） ——

    fn render_sidebar(&self, cx: &mut Context<Self>, id_prefix: &str) -> impl IntoElement {
        let theme = cx.theme();
        let active_nav = self.active_nav;

        // 顶部品牌区：Codex logo（用 Bot 图标代替）+ 名称
        let brand = h_flex()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(44.))
            .child(
                div()
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .bg(openai_green())
                    .text_color(rgb(0xffffff))
                    .child(Icon::new(IconName::Bot).small()),
            )
            .child(div().text_color(theme.foreground).child("Codex"));

        // New chat 按钮（主强调，整行宽）
        let new_chat = div().px_3().child(
            Button::new("sidebar-new-chat")
                .primary()
                .icon(IconName::Plus)
                .label("New chat")
                .w_full()
                .on_click(cx.listener(move |v, _, _, cx| {
                    v.active_nav = NavItem::NewChat;
                    cx.notify();
                })),
        );

        // 导航项列表
        let nav_items = [
            NavItem::Search,
            NavItem::Plugins,
            NavItem::Automations,
            NavItem::Pinned,
        ];
        let mut nav_list = v_flex().gap_0p5().px_3();
        for (i, &item) in nav_items.iter().enumerate() {
            let active = active_nav == item;
            let row = h_flex()
                .id(format!("nav-{}", i))
                .items_center()
                .gap_2()
                .px_2()
                .py_1p5()
                .rounded_md()
                .text_color(if active {
                    theme.foreground
                } else {
                    theme.muted_foreground
                })
                .bg(if active {
                    theme.accent.opacity(0.12)
                } else {
                    gpui::transparent_black()
                })
                .hover(|s| {
                    s.bg(theme.accent.opacity(0.08))
                        .text_color(theme.foreground)
                })
                .cursor_pointer()
                .child(Icon::new(item.icon()).small())
                .child(div().text_sm().child(item.label()))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |v, _, _, cx| {
                        v.active_nav = item;
                        cx.notify();
                    }),
                );
            nav_list = nav_list.child(row);
        }

        // Projects 区
        let mut projects_block = v_flex().gap_0p5().px_3().mt_4().child(
            div()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Projects"),
        );
        for (i, p) in self.projects.iter().enumerate() {
            projects_block = projects_block.child(
                h_flex()
                    .id(format!("proj-{}", i))
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .text_color(theme.foreground)
                    .hover(|s| s.bg(theme.accent.opacity(0.08)))
                    .cursor_pointer()
                    .child(div().size(px(8.)).rounded_full().bg(p.color))
                    .child(div().text_sm().child(p.name)),
            );
        }

        // Chats 历史区
        let mut chats_block = v_flex().gap_0p5().px_3().mt_4().child(
            div()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Chats"),
        );
        for (i, c) in self.chats.iter().enumerate() {
            chats_block = chats_block.child(
                h_flex()
                    .id(format!("chat-{}", i))
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .text_color(if c.active {
                        theme.foreground
                    } else {
                        theme.muted_foreground
                    })
                    .bg(if c.active {
                        theme.accent.opacity(0.12)
                    } else {
                        gpui::transparent_black()
                    })
                    .hover(|s| {
                        s.bg(theme.accent.opacity(0.08))
                            .text_color(theme.foreground)
                    })
                    .cursor_pointer()
                    .child(div().text_sm().child(c.title)),
            );
        }

        // 底部账号 + 设置
        let footer = h_flex()
            .items_center()
            .justify_between()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .size(px(28.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(rgb(0x6366f1))
                            .text_color(rgb(0xffffff))
                            .text_xs()
                            .child("A"),
                    )
                    .child(
                        v_flex()
                            .child(div().text_sm().child("alex@openai.com"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Pro"),
                            ),
                    ),
            )
            .child(
                Button::new("sidebar-settings")
                    .ghost()
                    .icon(IconName::Settings)
                    .tooltip("Settings"),
            );

        // 整个侧边栏：可滚动中间区 + 固定底部
        let sidebar = v_flex()
            .h_full()
            .w(px(264.))
            .flex_shrink_0()
            .bg(theme.title_bar)
            .border_r_1()
            .border_color(theme.border)
            .child(brand)
            .child(new_chat)
            .child(nav_list)
            .child(projects_block)
            .child(
                div()
                    .id("sidebar-chats-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(chats_block),
            )
            .child(footer);

        // 从左侧滑入
        sidebar
            .slide_right(format!("{}-sidebar", id_prefix), px(40.))
            .with_spec(AnimationSpec::default().with_duration(Duration::from_millis(450)))
    }

    // —— 主对话区（顶部栏使用 cx.listener，需 &mut；消息/composer 只读 theme） ——

    fn render_main(&self, cx: &mut Context<Self>, id_prefix: &str) -> impl IntoElement {
        let theme = cx.theme();
        let dark = self.dark;
        let right_panel_open = self.right_panel_open;
        let use_spring = self.use_spring;

        // 顶部栏：chat 标题 + 模型 badge + 右侧操作按钮
        let topbar = {
            let spec = AnimationSpec::default()
                .with_delay(Duration::from_millis(120))
                .with_duration(Duration::from_millis(350));
            h_flex()
                .items_center()
                .justify_between()
                .px_4()
                .h(px(52.))
                .border_b_1()
                .border_color(theme.border)
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(div().child("Refactor auth module"))
                        .child(
                            h_flex()
                                .items_center()
                                .gap_1()
                                .px_2()
                                .py_0p5()
                                .rounded_md()
                                .bg(theme.accent.opacity(0.12))
                                .text_color(openai_green())
                                .child(Icon::new(IconName::Bot).xsmall())
                                .child(div().text_xs().child("GPT-5.5")),
                        ),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_1()
                        .child(
                            Button::new("topbar-theme")
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
                            Button::new("topbar-panel")
                                .ghost()
                                .icon(if right_panel_open {
                                    IconName::PanelRight
                                } else {
                                    IconName::PanelRightOpen
                                })
                                .tooltip("Toggle right panel")
                                .on_click(cx.listener(move |v, _, _, cx| {
                                    v.right_panel_open = !v.right_panel_open;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("topbar-anim")
                                .ghost()
                                .label(if use_spring { "Spring" } else { "Easing" })
                                .tooltip("Toggle animation mode")
                                .on_click(cx.listener(move |v, _, _, cx| {
                                    v.use_spring = !v.use_spring;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("topbar-replay")
                                .primary()
                                .icon(IconName::Play)
                                .label("Replay")
                                .on_click(cx.listener(move |v, _, _, cx| {
                                    v.replay_count += 1;
                                    cx.notify();
                                })),
                        ),
                )
                .fade_in(format!("{}-topbar", id_prefix))
                .with_spec(spec)
        };

        // 消息流（只读 theme，不使用 cx.listener）
        let messages = self.render_messages(cx, id_prefix);
        let scroll = div()
            .id("codex-msg-scroll")
            .flex_1()
            .overflow_y_scroll()
            .child(
                v_flex()
                    .max_w(px(760.))
                    .mx_auto()
                    .py_6()
                    .px_6()
                    .gap_6()
                    .child(messages),
            );

        // 底部 composer
        let composer = self.render_composer(cx, id_prefix);

        v_flex()
            .h_full()
            .flex_1()
            .bg(theme.background)
            .child(topbar)
            .child(scroll)
            .child(composer)
    }

    // —— 消息流（只读，&Context） ——

    fn render_messages(&self, cx: &Context<Self>, id_prefix: &str) -> impl IntoElement {
        let p = id_prefix;

        // 第 1 条：用户消息
        let m1 = self.render_user_message(
            cx,
            format!("{}-m1", p),
            0,
            "帮我重构 auth 模块，改成 async/await 风格，并加上错误处理。",
        );

        // 第 2 条：assistant 回复（文本 + 代码块 + 工具调用 + diff + 收尾文本）
        let m2_text = "好的，我会把 `auth.rs` 里的同步函数改成 async，并引入 `thiserror` 做错误类型。先看一下当前实现：";
        let m2_children = vec![
            Self::msg_text(cx, m2_text),
            Self::msg_code_block(
                cx,
                "src/auth.rs",
                vec![
                    "pub fn login(email: &str, password: &str) -> Result<User, AuthError> {",
                    "    let user = db.find_user(email)?;",
                    "    verify_password(&password, &user.password_hash)?;",
                    "    Ok(user)",
                    "}",
                ],
            ),
            Self::msg_tool_call(cx, "cargo check", true),
            Self::msg_diff(
                cx,
                "src/auth.rs",
                vec![
                    "- pub fn login(email: &str, password: &str) -> Result<User, AuthError> {",
                    "+ pub async fn login(email: &str, password: &str) -> Result<User, AuthError> {",
                    "-     let user = db.find_user(email)?;",
                    "+     let user = db.find_user(email).await?;",
                ],
            ),
            Self::msg_text(
                cx,
                "改动很小：把 `db.find_user` 改成 `.await`，函数签名加 `async`。需要我顺手补一下测试吗？",
            ),
        ];
        let m2 = self.render_assistant_block(cx, format!("{}-m2", p), 120, m2_children);

        // 第 3 条：用户消息
        let m3 = self.render_user_message(cx, format!("{}-m3", p), 520, "好的，顺便加上。");

        // 第 4 条：assistant 正在思考
        let m4 = self.render_typing(cx, format!("{}-m4", p), 640);

        v_flex().gap_6().child(m1).child(m2).child(m3).child(m4)
    }

    /// 用户消息：右对齐气泡
    fn render_user_message(
        &self,
        cx: &Context<Self>,
        id: impl Into<gpui::ElementId>,
        delay_ms: u64,
        text: &str,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let spec = AnimationSpec::default()
            .with_delay(Duration::from_millis(delay_ms))
            .with_duration(Duration::from_millis(380));

        h_flex()
            .w_full()
            .justify_end()
            .child(
                div()
                    .max_w(px(560.))
                    .px_4()
                    .py_2p5()
                    .rounded_lg()
                    .bg(theme.accent.opacity(0.16))
                    .text_color(theme.foreground)
                    .text_sm()
                    .child(text.to_string()),
            )
            .slide_up(id, px(16.))
            .with_spec(spec)
    }

    /// assistant 区块：头像 + 名称 + 内容列表，整体 slide_up
    fn render_assistant_block(
        &self,
        cx: &Context<Self>,
        id: impl Into<gpui::ElementId>,
        delay_ms: u64,
        children: Vec<gpui::AnyElement>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let spec = AnimationSpec::default()
            .with_delay(Duration::from_millis(delay_ms))
            .with_duration(Duration::from_millis(420));

        h_flex()
            .gap_3()
            .items_start()
            .child(
                div()
                    .size(px(30.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(openai_green())
                    .text_color(rgb(0xffffff))
                    .child(Icon::new(IconName::Bot).small()),
            )
            .child(
                v_flex()
                    .flex_1()
                    .gap_3()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(div().text_sm().child("Codex"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("GPT-5.5"),
                            ),
                    )
                    .children(children),
            )
            .slide_up(id, px(20.))
            .with_spec(spec)
    }

    /// assistant 正在思考：spinner + 文案
    fn render_typing(
        &self,
        cx: &Context<Self>,
        id: impl Into<gpui::ElementId>,
        delay_ms: u64,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let spec = AnimationSpec::default()
            .with_delay(Duration::from_millis(delay_ms))
            .with_duration(Duration::from_millis(380));

        h_flex()
            .gap_3()
            .items_start()
            .child(
                div()
                    .size(px(30.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(openai_green())
                    .text_color(rgb(0xffffff))
                    .child(Icon::new(IconName::Bot).small()),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .rounded_lg()
                    .bg(theme.accent.opacity(0.06))
                    .text_color(theme.muted_foreground)
                    .child(gpui_component::spinner::Spinner::new().small())
                    .child(div().text_sm().child("Codex is working…")),
            )
            .slide_up(id, px(16.))
            .with_spec(spec)
    }

    // —— assistant 子元素工厂方法（只读 theme，&Context） ——

    fn msg_text(cx: &Context<Self>, text: &str) -> gpui::AnyElement {
        let theme = cx.theme();
        div()
            .text_color(theme.foreground)
            .text_sm()
            .child(text.to_string())
            .into_any_element()
    }

    /// 代码块：文件名 tab + 代码内容 + 复制按钮
    fn msg_code_block(cx: &Context<Self>, file: &str, lines: Vec<&str>) -> gpui::AnyElement {
        let theme = cx.theme();
        let code_lines = lines
            .iter()
            .map(|l| {
                div()
                    .child(SharedString::from(format!("{}\n", l)))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        v_flex()
            .w_full()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_1p5()
                    .bg(theme.title_bar)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(Icon::new(IconName::File).xsmall())
                            .child(div().text_xs().child(file.to_string())),
                    )
                    .child(
                        Button::new(format!("copy-{}", file))
                            .ghost()
                            .small()
                            .icon(IconName::Copy)
                            .tooltip("Copy"),
                    ),
            )
            .child(
                v_flex()
                    .px_3()
                    .py_2()
                    .gap_0()
                    .bg(theme.background)
                    .font_family("monospace")
                    .text_xs()
                    .text_color(theme.foreground)
                    .children(code_lines),
            )
            .into_any_element()
    }

    /// 工具调用卡片：图标 + 标题 + 命令 + 状态
    fn msg_tool_call(cx: &Context<Self>, command: &str, done: bool) -> gpui::AnyElement {
        let theme = cx.theme();
        let status_icon: gpui::AnyElement = if done {
            Icon::new(IconName::CircleCheck)
                .xsmall()
                .text_color(theme.green)
                .into_any_element()
        } else {
            gpui_component::spinner::Spinner::new()
                .small()
                .into_any_element()
        };

        v_flex()
            .w_full()
            .gap_2()
            .px_3()
            .py_2p5()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.accent.opacity(0.04))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(Icon::new(IconName::Frame).xsmall())
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Ran shell command"),
                            ),
                    )
                    .child(status_icon),
            )
            .child(
                v_flex()
                    .px_2p5()
                    .py_2()
                    .rounded_md()
                    .bg(theme.background)
                    .font_family("monospace")
                    .text_xs()
                    .text_color(theme.foreground)
                    .child(SharedString::from(format!(
                        "$ {}\n    Finished `dev` profile in 0.82s",
                        command
                    ))),
            )
            .into_any_element()
    }

    /// diff 卡片：文件名 + 增删行（红 / 绿）
    fn msg_diff(cx: &Context<Self>, file: &str, lines: Vec<&str>) -> gpui::AnyElement {
        let theme = cx.theme();
        let diff_lines = lines
            .iter()
            .map(|l| {
                let is_add = l.starts_with('+');
                let is_del = l.starts_with('-');
                let line_bg = if is_add {
                    theme.green.opacity(0.12)
                } else if is_del {
                    theme.red.opacity(0.12)
                } else {
                    gpui::transparent_black()
                };
                let line_fg = if is_add {
                    theme.green
                } else if is_del {
                    theme.red
                } else {
                    theme.foreground
                };
                div()
                    .px_3()
                    .py_0p5()
                    .bg(line_bg)
                    .text_color(line_fg)
                    .child(SharedString::from(format!("{}\n", l)))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        v_flex()
            .w_full()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1p5()
                    .bg(theme.title_bar)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(Icon::new(IconName::File).xsmall())
                    .child(div().text_xs().child(file.to_string())),
            )
            .child(
                v_flex()
                    .py_2()
                    .font_family("monospace")
                    .text_xs()
                    .children(diff_lines),
            )
            .into_any_element()
    }

    // —— composer（只读，&Context；send 按钮用自由闭包而非 cx.listener） ——

    fn render_composer(&self, cx: &Context<Self>, id_prefix: &str) -> impl IntoElement {
        let theme = cx.theme();
        let spec = AnimationSpec::default()
            .with_delay(Duration::from_millis(600))
            .with_duration(Duration::from_millis(420));

        let bar = h_flex()
            .items_end()
            .gap_2()
            .mx_auto()
            .max_w(px(760.))
            .w_full()
            .px_4()
            .py_3()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.title_bar)
            .child(
                Button::new("composer-add")
                    .ghost()
                    .icon(IconName::Plus)
                    .tooltip("Add file / plan mode"),
            )
            .child(div().flex_1().child(Input::new(&self.input).w_full()))
            .child(
                Button::new("composer-model")
                    .ghost()
                    .icon(IconName::Bot)
                    .label("GPT-5.5")
                    .tooltip("Model"),
            )
            .child(
                Button::new("composer-quality")
                    .ghost()
                    .label("High")
                    .tooltip("Reasoning effort"),
            )
            .child(
                Button::new("composer-send")
                    .primary()
                    .icon(IconName::ArrowUp)
                    .tooltip("Send")
                    .on_click(|_, window, cx| {
                        window.push_notification("Codex is thinking…", cx);
                    }),
            );

        v_flex().px_6().pb_4().child(
            bar.slide_up(format!("{}-composer", id_prefix), px(24.))
                .with_spec(spec),
        )
    }

    // —— 右侧面板（标签栏使用 cx.listener，需 &mut；内容区只读） ——

    fn render_right_panel_inner(
        &self,
        entity: gpui::Entity<Self>,
        _window: &mut Window,
        cx: &App,
    ) -> gpui::Div {
        let theme = cx.theme();
        let active_tab = self.right_panel_tab;
        let tabs = [
            RightPanelTab::Browser,
            RightPanelTab::Files,
            RightPanelTab::Terminal,
            RightPanelTab::Summary,
        ];

        // 标签栏
        let mut tab_bar = h_flex()
            .items_center()
            .justify_between()
            .px_2()
            .h(px(44.))
            .border_b_1()
            .border_color(theme.border)
            .child(h_flex().items_center().gap_0p5());
        for (i, &t) in tabs.iter().enumerate() {
            let active = active_tab == t;
            let tab_entity = entity.clone();
            let tab = h_flex()
                .id(format!("tab-{}", i))
                .items_center()
                .gap_1p5()
                .px_2p5()
                .py_1p5()
                .rounded_md()
                .text_color(if active {
                    theme.foreground
                } else {
                    theme.muted_foreground
                })
                .bg(if active {
                    theme.accent.opacity(0.12)
                } else {
                    gpui::transparent_black()
                })
                .hover(|s| {
                    s.bg(theme.accent.opacity(0.08))
                        .text_color(theme.foreground)
                })
                .cursor_pointer()
                .child(Icon::new(t.icon()).xsmall())
                .child(div().text_xs().child(t.label()))
                .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                    tab_entity.update(cx, |v, cx| {
                        v.right_panel_tab = t;
                        cx.notify();
                    });
                });
            tab_bar = tab_bar.child(tab);
        }
        let close_entity = entity.clone();
        tab_bar = tab_bar.child(
            Button::new("panel-close")
                .ghost()
                .small()
                .icon(IconName::Close)
                .tooltip("Close panel")
                .on_click(move |_, _, cx| {
                    close_entity.update(cx, |v, cx| {
                        v.right_panel_open = false;
                        cx.notify();
                    });
                }),
        );

        // 内容区（根据 tab 切换）
        let content: gpui::Div = match active_tab {
            RightPanelTab::Browser => self.render_panel_browser(cx),
            RightPanelTab::Files => self.render_panel_files(cx),
            RightPanelTab::Terminal => self.render_panel_terminal(cx),
            RightPanelTab::Summary => self.render_panel_summary(cx),
        };

        let panel = v_flex()
            .h_full()
            .w(px(340.))
            .flex_shrink_0()
            .bg(theme.title_bar)
            .border_l_1()
            .border_color(theme.border)
            .child(tab_bar)
            .child(
                div()
                    .id("panel-content-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(content),
            );

        // 内容区直接放入 scroll 容器，不再单独包 fade_in（Presence 处理面板级动画）
        panel
    }

    /// 右侧面板 - Browser 内容：模拟内嵌浏览器
    fn render_panel_browser(&self, cx: &App) -> gpui::Div {
        let theme = cx.theme();
        v_flex()
            .gap_3()
            .p_3()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.background)
                    .child(Icon::new(IconName::Globe).xsmall())
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("http://localhost:3000"),
                    )
                    .child(div().flex_1())
                    .child(Icon::new(IconName::ArrowLeft).xsmall())
                    .child(Icon::new(IconName::ArrowRight).xsmall())
                    .child(Icon::new(IconName::Replace).xsmall()),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(360.))
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.background)
                    .p_6()
                    .child(
                        v_flex()
                            .gap_3()
                            .items_start()
                            .child(
                                div()
                                    .text_xl()
                                    .text_color(theme.foreground)
                                    .child("My Landing Page"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child("Built with Codex · deployed to Vercel"),
                            )
                            .child(
                                div()
                                    .px_4()
                                    .py_2()
                                    .rounded_md()
                                    .bg(openai_green())
                                    .text_color(rgb(0xffffff))
                                    .text_sm()
                                    .child("Get Started"),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .h(px(120.))
                                    .rounded_md()
                                    .bg(theme.accent.opacity(0.12))
                                    .mt_4(),
                            ),
                    ),
            )
    }

    /// 右侧面板 - Files 内容
    fn render_panel_files(&self, cx: &App) -> gpui::Div {
        let theme = cx.theme();
        let files = [
            "src/auth.rs",
            "src/main.rs",
            "src/db.rs",
            "Cargo.toml",
            "README.md",
        ];
        let mut list = v_flex().p_3().gap_0p5();
        for (i, f) in files.iter().enumerate() {
            list = list.child(
                h_flex()
                    .id(format!("file-{}", i))
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .hover(|s| s.bg(theme.accent.opacity(0.08)))
                    .cursor_pointer()
                    .child(Icon::new(IconName::File).xsmall())
                    .child(div().text_sm().child(*f)),
            );
        }
        list
    }

    /// 右侧面板 - Terminal 内容
    fn render_panel_terminal(&self, cx: &App) -> gpui::Div {
        let theme = cx.theme();
        v_flex()
            .p_3()
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .px_2p5()
                            .py_1()
                            .rounded_md()
                            .bg(theme.accent.opacity(0.12))
                            .text_color(theme.foreground)
                            .text_xs()
                            .child("powershell"),
                    )
                    .child(
                        Button::new("term-new")
                            .ghost()
                            .small()
                            .icon(IconName::Plus)
                            .tooltip("New terminal"),
                    ),
            )
            .child(
                v_flex()
                    .p_3()
                    .rounded_md()
                    .bg(theme.background)
                    .font_family("monospace")
                    .text_xs()
                    .text_color(theme.foreground)
                    .gap_0p5()
                    .child(SharedString::from("PS C:\\wb-gpui> cargo check"))
                    .child(SharedString::from(
                        "    Compiling gpui-component-motion v0.1.0",
                    ))
                    .child(SharedString::from("    Finished `dev` profile in 0.82s"))
                    .child(SharedString::from("PS C:\\wb-gpui> _")),
            )
    }

    /// 右侧面板 - Summary 内容：agent 计划 / 来源 / 产物
    fn render_panel_summary(&self, cx: &App) -> gpui::Div {
        let theme = cx.theme();
        v_flex()
            .p_3()
            .gap_4()
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Plan"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.foreground)
                            .child("Refactor auth module to async/await"),
                    )
                    .child(
                        v_flex().gap_1().mt_2().children(
                            [
                                "Convert login to async",
                                "Add thiserror error type",
                                "Update call sites to .await",
                            ]
                            .iter()
                            .map(|s| {
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        Icon::new(IconName::CircleCheck)
                                            .xsmall()
                                            .text_color(theme.green),
                                    )
                                    .child(div().text_xs().child(*s))
                            }),
                        ),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Sources"),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(Icon::new(IconName::File).xsmall())
                            .child(div().text_sm().child("src/auth.rs")),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(Icon::new(IconName::File).xsmall())
                            .child(div().text_sm().child("src/db.rs")),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Artifacts"),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .px_2p5()
                            .py_1p5()
                            .rounded_md()
                            .bg(theme.accent.opacity(0.08))
                            .child(Icon::new(IconName::File).xsmall())
                            .child(div().text_sm().child("auth.patch")),
                    ),
            )
    }
}

impl Render for CodexApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let id_prefix = format!("codex-{}", self.replay_count);
        let right_panel_open = self.right_panel_open;
        let use_spring = self.use_spring;

        // 根据 use_spring 构建 lifecycle：Spring 物理缓动 vs 传统 EaseOut
        let lifecycle = if use_spring {
            MotionLifecycle::expand_width(
                px(340.),
                AnimationSpec::default().with_spring(SpringPreset::Default),
            )
            .with_exit_spec(AnimationSpec::default().with_duration(Duration::from_millis(250)))
        } else {
            MotionLifecycle::expand_width(
                px(340.),
                AnimationSpec::default().with_duration(Duration::from_millis(300)),
            )
            .with_exit_spec(AnimationSpec::default().with_duration(Duration::from_millis(250)))
        };

        // 更新 Presence 动画配对 + 期望状态
        self.right_panel_presence.update(cx, |s, presence_cx| {
            s.set_lifecycle(lifecycle, presence_cx);
            s.set_present(right_panel_open, _window, presence_cx);
        });

        h_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_sidebar(cx, &id_prefix))
            .child(self.render_main(cx, &id_prefix))
            .child(self.right_panel_presence.clone())
    }
}

fn run_example() {
    application().with_assets(Assets).run(|cx: &mut App| {
        gpui_component::init(cx);
        // Codex 默认深色主题
        Theme::change(ThemeMode::Dark, None, cx);
        cx.activate(true);

        let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| CodexApp::new(window, cx));
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
