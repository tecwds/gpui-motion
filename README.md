# gpui-component-motion

为 [gpui-component](https://github.com/longbridge/gpui-component) 提供的非侵入式动画层。

在 GPUI 的 `with_animation` 基座之上封装了预设动效（Fade / Slide / ExpandWidth / ExpandHeight）、
Spring 物理缓动、声明式生命周期容器（PresenceState）与元素扩展 trait（MotionExt），
让组件的入场 / 退场动画只需一行链式调用即可接入，无需手动管理 `Animation` 状态。

## 特性

- **非侵入式**：通过 blanket impl 的 `MotionExt` trait 为所有 `IntoElement + Styled` 类型添加动画方法，不修改原有组件代码。
- **入场 + 退场配对**：`MotionLifecycle` 描述元素的 mount / unmount 动效，`PresenceState` 自动管理时序与卸载。
- **Spring 物理动画**：基于阻尼振荡方程的 `SpringPreset`（Stiff / Default / Gentle / Wobbly），支持过冲效果，比传统二次曲线更自然。**退场强制剥离 Spring**（任何构造路径），保证退场曲线严格单调。
- **布局动画**：`ExpandWidth` / `ExpandHeight` 驱动元素尺寸从 0 到目标值渐变，适用于面板展开 / 折叠 / dropdown / accordion。
- **reduce_motion 友好**：内部复用 GPUI `AnimationElement`，系统启用减弱动画时自动渲染结束帧。
- **低分配**：`Easing` / `SpringPreset` / `AnimationSpec` / `Motion` 均为 `Copy`；`Animation` 在 builder 阶段预构建，渲染期仅浅拷贝 `Rc`（每帧仅剩 `with_animation` 签名强制的一次 `Box` 分配）。
- **`ParentElement` 支持**：`Animated<T>` 实现 `ParentElement`，可对其直接 `.child(...)` / `.children(...)`。

## 快速上手

### 1. 入场动画

为任意元素添加入场动效，只需调用 `MotionExt` 方法并传入唯一 `ElementId`：

```rust
use gpui_component_motion::MotionExt;
use gpui::{div, px, Styled};

div()
    .child(div().fade_in("my-fade"))
    .child(div().slide_up("my-slide", px(10.0)));
```

自定义时长、延迟与缓动：

```rust
use std::time::Duration;
use gpui_component_motion::{AnimationSpec, Easing, MotionExt};
use gpui::{div, px, Styled};

div().fade_in("my-fade").with_spec(
    AnimationSpec::default()
        .with_duration(Duration::from_millis(400))
        .with_delay(Duration::from_millis(50))
        .with_easing(Easing::EaseInOut),
);
```

### 2. Spring 物理缓动

Spring 基于阻尼振荡方程，输出可超过 1（过冲），比传统 EaseOut 二次曲线更自然。
启用后自动设置推荐时长，也可后续 `with_duration` 覆盖：

```rust
use gpui_component_motion::{AnimationSpec, SpringPreset, MotionExt};
use gpui::{div, Styled};

// 轻微过冲（通用入场）
div().fade_in("my-spring").with_spec(
    AnimationSpec::default().with_spring(SpringPreset::Default),
);

// 明显弹跳（pop 效果）
div().fade_in("my-pop").with_spec(
    AnimationSpec::default().with_spring(SpringPreset::Wobbly),
);
```

> **架构说明**：GPUI 内部 `debug_assert` 要求 easing 输出 ∈ `[0, 1]`，
> 而 Spring 过冲需要 > 1。解决方案是 `Animated` 在 Spring 模式下将 GPUI
> easing 设为 `Linear`（原样传递线性进度），在 animator 回调内部调用
> `spring.curve(t)` 做物理映射，绕过约束。

### 3. 声明式 Presence（入场 + 退场）

`PresenceState` 是一个 `Entity<PresenceState>`，调用方只声明"元素何时该存在"，
框架自动播入场 / 退场动画并在退场结束后卸载元素：

```rust
use std::time::Duration;
use gpui_component_motion::{AnimationSpec, MotionLifecycle, PresenceState, SpringPreset};
use gpui::{div, px, Styled};

// 一次创建
let presence = PresenceState::new(
    cx,
    "right-panel",
    MotionLifecycle::expand_width(
        px(340.),
        // 入场用 Spring 物理缓动
        AnimationSpec::default().with_spring(SpringPreset::Default),
    )
    // 退场用传统 EaseOut（退场强制剥离 Spring；此处只覆盖时长）
    .with_exit_spec(AnimationSpec::default().with_duration(Duration::from_millis(250))),
    move |window, cx| {
        // 每帧调用以重建子元素
        div().w(px(340.)).h_full().child("panel content")
    },
);

// 每帧更新期望状态
presence.update(cx, |s, cx| {
    s.set_present(is_open, window, cx);
});

// 运行时切换动画模式（如 Spring ↔ Easing 对比）
presence.update(cx, |s, cx| {
    s.set_lifecycle(new_lifecycle, cx);
});

// 插入到元素树
h_flex().child(presence.clone());
```

状态机：

```
HIDDEN ──set_present(true)──▶ ENTERING(epoch+=1, enter_active=快照)
ENTERING ──set_present(false)──▶ EXITING(epoch+=1, exit_active=快照, 定时器=total+GRACE)
VISIBLE ──set_present(false)──▶ EXITING(同上)
EXITING ──set_present(true)──▶ ENTERING(epoch+=1, 取消定时器, exit_active=None)
EXITING ──定时器到期且 epoch 匹配──▶ HIDDEN(closing=false, visible=false, 双 active=None)
EXITING ──定时器到期但 epoch 不匹配──▶ 忽略（无副作用）
```

> **关键**：每次状态变迁（入场、退场、中断退场重入场）都会递增 `epoch`，
> 保证动画 `ElementId` 全局唯一，避免 GPUI `AnimationState` 缓存命中
> 已完成的 `delta=1.0` 状态导致"无动画瞬现"。

> **过渡期快照**：状态转换瞬间捕获入场 / 退场动效与规格（`enter_active` / `exit_active`），
> 进行中的动画不受后续 `set_lifecycle` 影响；`set_lifecycle` 传入相等 lifecycle 时直接短路
> （不通知、不触碰快照），仅对后续过渡生效。

> **卸载宽限期**：退场动画结束后额外等待 50ms（`total + GRACE`）再卸载元素，
> 兜底 GPUI 动画起点在元素首次 layout 才打点的滞后（至多一帧）。

## 架构

```
┌─────────────────────────────────────────────────────┐
│                  调用方代码                          │
│   MotionExt::fade_in / slide_up / ...               │
│   PresenceState::set_present / set_lifecycle        │
└──────────┬──────────────────────────┬───────────────┘
           ▼                          ▼
   ┌──────────────┐          ┌──────────────────┐
   │ Animated<T>  │          │ PresenceState    │
   │  元素包装器   │          │  生命周期容器     │
   │  enter/exit  │          │  HIDDEN→ENTERING │
   │  Spring 映射  │          │  →VISIBLE→EXITING│
   └──────┬───────┘          └────────┬─────────┘
          │                           │ 每帧重建 child
          ▼                           ▼
   ┌──────────────────────────────────────┐
   │            MotionLifecycle           │
   │   enter: Motion + AnimationSpec      │
   │   exit:  Motion + AnimationSpec      │
   └──────────────┬───────────────────────┘
                  ▼
   ┌──────────────────────────────────────┐
   │     Motion  (Fade / Slide* /         │
   │     ExpandWidth / ExpandHeight)      │
   │     apply(el, t) → Styled            │
   └──────────────┬───────────────────────┘
                  ▼
   ┌──────────────────────────────────────┐
   │  AnimationSpec  (duration + delay +  │
   │  Easing + Option<SpringPreset>)      │
   └──────────────┬───────────────────────┘
                  ▼
   ┌──────────────────────────────────────┐
   │  GPUI with_animation / AnimationEl   │
   │  (reduce_motion 检测、帧调度)         │
   └──────────────────────────────────────┘
```

| 模块 | 职责 |
|------|------|
| [`Easing`](src/easing.rs) | 传统缓动曲线（Linear / EaseIn / EaseOut / EaseInOut），输出 ∈ [0, 1] |
| [`SpringPreset`](src/easing.rs) | Spring 物理缓动（Stiff / Default / Gentle / Wobbly），基于阻尼振荡闭式解，输出可 > 1（过冲） |
| [`AnimationSpec`](src/spec.rs) | 时长 + 延迟 + 缓动 + Spring，含 `fast` / `default` / `slow` 预设与 `with_spring` builder |
| [`Motion`](src/motion.rs) | 预设动效（Fade / SlideUp / SlideDown / SlideLeft / SlideRight / ExpandWidth / ExpandHeight） |
| [`Animated<T>`](src/animated.rs) | 元素包装器，实现 `IntoElement`，预设起始态、Spring 物理映射、委托 GPUI 动画 |
| [`MotionExt`](src/ext.rs) | blanket impl，为 `IntoElement + Styled` 提供链式快捷方法 |
| [`MotionLifecycle`](src/lifecycle.rs) | 入场 / 退场动效配对，含 `expand_width` / `expand_height` / `fade` / `slide_*` 预设 |
| [`PresenceState`](src/presence.rs) | 声明式生命周期容器，自动管理入场 / 退场时序与卸载，支持 `set_lifecycle` 运行时切换 |

## 预设动效

| Motion | 入场效果 | 退场效果 | 适用场景 |
|--------|---------|---------|---------|
| `Fade` | 透明度 0 → 1 | 透明度 1 → 0 | 通用淡入淡出 |
| `SlideUp(offset)` | top: +offset → 0 | top: 0 → +offset | 从下方滑入 |
| `SlideDown(offset)` | top: -offset → 0 | top: 0 → -offset | 从上方滑入 |
| `SlideLeft(offset)` | left: +offset → 0 | left: 0 → +offset | 从右侧滑入（右侧面板） |
| `SlideRight(offset)` | left: -offset → 0 | left: 0 → -offset | 从左侧滑入（左侧面板） |
| `ExpandWidth(max)` | width: 0 → max | width: max → 0 | 水平面板展开 / 收起 |
| `ExpandHeight(max)` | height: 0 → max | height: max → 0 | 垂直折叠（dropdown / accordion / collapse） |
| `BackgroundColor(from, to)` | 背景色 from → to | 背景色 to → from | 强调色渐变（如红→蓝）、状态着色 |
| `TextColor(from, to)` | 文字色 from → to | 文字色 to → from | 文字高亮、状态文字变色 |
| `BorderColor(from, to)` | 边框色 from → to | 边框色 to → from | 选中 / 校验状态的边框高亮 |

## 动画规格预设

### 传统缓动

| 预设 | 时长 | 缓动 | 适用场景 |
|------|------|------|---------|
| `AnimationSpec::fast()` | 120ms | EaseOut | 微交互（按钮反馈、tooltip） |
| `AnimationSpec::default()` | 200ms | EaseOut | 通用入场动画 |
| `AnimationSpec::slow()` | 350ms | EaseInOut | 大面积过渡（面板、页面切换） |

### Spring 物理缓动

| 预设 | 阻尼比 ζ | 频率 ω₀ | 推荐时长 | 过冲 | 适用场景 |
|------|---------|---------|---------|------|---------|
| `SpringPreset::Stiff` | 1.0 | 35 | 186ms | 无 | 临界阻尼，快速无过冲（退场、折叠） |
| `SpringPreset::Default` | 0.7 | 30 | 219ms | 轻微 | 通用入场 |
| `SpringPreset::Gentle` | 0.5 | 25 | 368ms | 温和 | 大面积过渡 |
| `SpringPreset::Wobbly` | 0.3 | 28 | 548ms | 明显 | 活泼 pop 效果 |

> **退场强制剥离 Spring**：所有退场规格构造路径（`Animated::new` / `Animated::with_exit` /
> `MotionLifecycle::new` / `with_exit_spec`）都会剥离 Spring——退场过冲到负值对
> width / opacity 无意义，且反向 Spring 无法精确回到起始态。若原规格时长未被显式覆盖
> （仍等于该预设的推荐时长），剥离后自动重置为默认 200ms；显式时长则保留。
> 剥离逻辑封装为公开方法 `AnimationSpec::without_spring()`，手动构造场景亦可复用。
> 如需更长的退场动画，直接通过 `with_duration` 定制即可。

## 组合动效（Phase 1）

Phase 1 新增三组能力：颜色插值（`BackgroundColor` / `TextColor` / `BorderColor`）、
多段关键帧（`MotionKeyframes`）与级联入场（`stagger`），可组合出更丰富的入场效果。

### 颜色插值

颜色变体携带一对 `Hsla`（`hsla(h, s, l, a)`，各通道 ∈ [0, 1]），入场时从 `from`
渐变到 `to`（色相走最短路径，避免绕色环长路导致中间色相偏离直觉）：

```rust
use gpui_component_motion::{AnimationSpec, Motion, MotionExt};
use gpui::{div, hsla, px, Styled};
use std::time::Duration;

// 背景色红 → 蓝，600ms
div()
    .w(px(120.))
    .h(px(80.))
    .rounded_lg()
    .with_motion(
        "color-bg",
        AnimationSpec::default().with_duration(Duration::from_millis(600)),
        Motion::BackgroundColor(hsla(0.0, 0.9, 0.5, 1.0), hsla(0.6, 0.9, 0.5, 1.0)),
    );
```

`TextColor` / `BorderColor` 用法相同，分别渐变 `text_color` / `border_color`；
退场时反向（`to → from`）渐变。

### 关键帧动画（Keyframes）

`MotionKeyframes` 将总时长按各段 `ratio` 拆分为多段子动画顺序播放
（`ratio` 为相对权重，构建时按全部帧归一化；每段至少 1ms，末段吸收舍入余数）：

```rust
use gpui_component_motion::{Easing, Motion, MotionKeyframes};
use gpui::{div, px, Styled};
use std::time::Duration;

// 总时长 600ms：前 50% 淡入，后 50% 上滑 16px
MotionKeyframes::new(div(), "kf-demo", Duration::from_millis(600))
    .keyframe(1.0, Easing::EaseOut, Motion::Fade)
    .keyframe(1.0, Easing::EaseOut, Motion::SlideUp(px(16.0)));
```

### 级联入场（Stagger）

`stagger` 为一组 `Animated` 元素按索引递增延迟（第 `i` 个延迟 `gap * i`），
适合列表 / 网格的级联入场；返回的 `Vec<Animated<T>>` 可直接 `.children(...)`
挂进容器：

```rust
use gpui_component_motion::{MotionExt, stagger};
use gpui::{div, Styled};
use std::time::Duration;

let items = vec![
    div().fade_in("item-0"),
    div().fade_in("item-1"),
    div().fade_in("item-2"),
];
// 延迟 0ms / 80ms / 160ms，级联入场
let staggered = stagger(items, Duration::from_millis(80));
```

## 已知限制

- **打断跳变（S8）**：GPUI `Animation` 不支持自定义起始进度，退场→入场 / 入场→退场
  打断时存在一次可见跳变（从当前帧位置直接跳到新动画的起点）。
- **Expand 重排成本（S10）**：`ExpandWidth` / `ExpandHeight` 每帧改变尺寸，
  触发 taffy 子树重排与文本重排版，大文本子树慎用。
- **delay 重绘成本（S11）**：`delay` 折叠进缓动前缀，GPUI `AnimationElement` 在
  `done` 前每帧请求重绘，延迟期仍全速重绘，长 delay（>300ms）需知悉成本。
- **终态样式覆盖**：动画在 `t=1` 应用 Motion 终态样式，覆盖元素上与该动效冲突的
  既有样式（GPUI `Styled` refinement 不可移除、不可读回，属框架限制）；
  建议将动画应用于包装元素而非直接动画元素本身。
- **零时长安全**：`duration + delay == 0` 时自动钳制为 1ms 最小动画时长，
  全路径无 NaN / 除零（纯 delay 场景延迟期输出起始态、`t=1` 输出终态）。
- **`MotionExt` 全局独占**：`MotionExt` 为 blanket impl，下游不可再为具体类型实现该
  trait，且 `fade_in` / `slide_up` / `with_motion` 等方法名在依赖图中全局抢占——属
  有意设计（一行接入的代价，详见 `ext.rs` 模块文档）。

## 性能

本库运行在 GPUI 的"每帧重建"模型之上，以下成本属框架特性，按需知悉与规避：

- **每帧成本随树规模线性增长**：GPUI 每次重绘都会重跑 `render`、重建整棵元素树并全量
  重算 taffy 布局（`ViewElement::request_layout` 无条件重跑 render；布局引擎每次 draw
  全量清空重排）。任一动画活跃期间整窗每帧都执行上述流程，树越大、每帧成本越高。
- **常驻 `repeat()` 组件会钉住整窗满帧重绘**：`Spinner` / `ProgressCircle::loading(true)`
  等基于 `Animation::repeat()` 的组件永不结束，每帧请求下一帧，驱动窗口以满刷新率持续
  重绘**整个会话**（空闲也烧满 CPU/GPU）。请用状态标志（如 `loading`）条件挂载，
  完成后立即卸载，空闲时不展示。
- **`delay` 会延长重绘窗口**：`delay` 折叠进缓动前缀，`AnimationElement` 在延迟期仍每帧
  tick——子树每帧全量重排（layout + paint）+ 驱动整窗重绘。长 delay（>300ms）需知悉成本。
- **render 保持零分配**：render 每帧执行，其中的任何堆分配都是每帧成本。避免在 render
  内用 `format!` 拼接 id / 文本（随条目数线性增长），改用稳定 `ElementId` 基名 + 计数
  （`ElementId::NamedInteger` / `named_usize`）与静态字符串；逐行文本在 render 外预构建复用。
- **低成本范本**：`examples/story.rs` 是刻意保持最小开销的对照实现——纯静态内容、
  无常驻动画、文本零分配（仅 9 张静态卡片的 id 各 1 次 `format!`），作为衡量真实成本的下限参考。

## Examples

```sh
# Codex 桌面版 UI（三列布局 + 右侧面板 Presence 动画 + Spring/Easing 切换按钮）
just example codex

# 组件动画展示
just example gallery

# 故事板
just example story
```

## 开发

项目使用 [just](https://github.com/casey/just) 管理常用任务（Windows 上使用 PowerShell）：

```sh
just              # 列出所有任务
just check        # cargo check --all-targets
just test         # cargo test
just fmt          # cargo fmt
just lint         # cargo clippy --all-targets -- -D warnings
just ci           # 格式检查 + clippy + 测试（一键本地 CI）
```

不使用 just 时可直接运行 cargo 命令：

```sh
cargo test -p gpui-component-motion
cargo clippy --all-targets -- -D warnings
cargo run --example codex
```

## 依赖

- [gpui](https://github.com/zed-industries/zed) — Zed 的 GPU 加速 UI 框架
- [gpui-component](https://github.com/longbridge/gpui-component) — gpui 组件库

## License

MIT
