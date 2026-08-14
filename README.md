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

## 循环动效与数值弹簧（Phase 2）

Phase 2 新增两组能力：永不结束的循环动效（`LoopMotion`，C7）与声明式数值弹簧
（`SpringValue`，D8）。二者互不依赖，可独立使用。

### 循环动效（LoopMotion）

`LoopMotion` 为元素挂载一个永不结束的循环动画，基于 GPUI `repeat_synced`
（相位锁到 App 共享时钟，所有循环由同一次帧调度驱动），作用于元素整体透明度（含子元素）：

| 预设 | 波形 | 透明度曲线 | 适用场景 |
|------|------|-----------|---------|
| `LoopKind::Pulse` | 抛物线（平滑呼吸） | 0.4 → 1.0 → 0.4 | 进行中 / 待机提示、呼吸强调 |
| `LoopKind::Skeleton` | 三角波（闪烁） | 0.5 → 1.0 → 0.5 | 骨架屏占位（近似 shimmer） |

```rust
use std::time::Duration;
use gpui_component_motion::{LoopKind, LoopMotion};
use gpui::{div, px, Styled};

// Pulse 平滑呼吸：透明度 0.4 → 1.0 → 0.4（900ms 周期）
let pulse = LoopMotion::pulse(
    div().w(px(120.)).h(px(80.)).rounded_lg().bg(hsla(0.6, 0.9, 0.5, 1.0)),
    "pulse-demo",
    Duration::from_millis(900),
);

// Skeleton 骨架屏闪烁：透明度 0.5 → 1.0 → 0.5（800ms 周期）
let skeleton = LoopMotion::skeleton(
    div().child("placeholder"),
    "skeleton-demo",
    Duration::from_millis(800),
);
```

> **⚠️ 必须条件挂载（E1/E2）**：`repeat_synced` 循环**永不结束、每帧 tick** —— 只要
> 元素保持挂载，窗口就会以满刷新率持续重绘整个会话（对照「性能」节：常驻 `repeat()`
> 组件钉住整窗满帧重绘）。请**按需挂载**：用状态标志（如 `loading` / `pulsing`）
> 条件挂载，效果结束时立即卸载 —— **卸载即停**，动画状态随元素销毁，无后台残留；
> 空闲时不挂载。Skeleton 为近似 shimmer 的三角波闪烁（GPUI 暂无渐变位置样式，
> 真 shimmer 留待上游支持）。

### 数值弹簧（SpringValue）

`SpringValue` 是一个 `Entity<SpringValue>`：调用方只声明"数值应当变成多少"
（`set_target`），内部以 16ms ≈ 60fps 的固定帧率驱动插值，每帧 `notify` 订阅者，
由订阅者在 render 中读取 `value()` 渲染。典型用途：数字滚动 / 计数器、进度条与
加载指示器、开关滑块回弹、图表数据点过渡。

```rust
use gpui_component_motion::{SpringPreset, SpringValue};

// 一次创建：初始值 0，Wobbly 预设（明显振荡、可过冲）
let value = SpringValue::new(cx, 0.0, SpringPreset::Wobbly);

// 事件驱动：声明新目标 —— 弹簧从当前值插值到 100
value.update(cx, |s, cx| s.set_target(100.0, window, cx));

// render 中读取当前值（渲染期只读；每帧 tick notify 驱动重绘）
let current = value.read(cx).value();
```

- **与 `SpringPreset` 曲线完全一致**：插值为 `from + (to - from) * progress(t)`，
  `t ≥ 1` 精确收敛到目标值（无残差）；欠阻尼预设（Default / Gentle / Wobbly）
  中间允许过冲（瞬时值可越出 `[min(from, to), max(from, to)]`）。
- **重定向从当前值起跳**：动画中途再次 `set_target` 时 `from = current`
  （不做速度衔接），旧 tick 任务被丢弃，世代守卫防止过期 tick 污染新动画。
- **短路**：目标未变且无进行中动画时不启动新 tick、不 `notify`。
- 渲染端每帧读取 `value()`，勿在 render 内调用 `set_target`。

## 多子元素进出与手势拖拽（Phase 3）

Phase 3 新增两组能力：多子元素声明式进出容器（`PresenceSet`，E10）与手势驱动弹簧
（`DragSpring`，D9）。二者互不依赖，可独立使用。

### 多子元素进出（PresenceSet）

`PresenceSet` 是 `PresenceState` 的多 key 泛化：按 `SharedString` key 管理任意数量
的 child，每个 key 拥有独立、互不干扰的进出场生命周期。典型场景：列表项增删、标签页
切换、通知栈。

```rust
use gpui_component_motion::{AnimationSpec, MotionLifecycle, PresenceSet};
use gpui::{div, px, Styled};

// 一次创建：builder 每帧按 key 重建 child（应捕获 WeakEntity 读取最新状态，避免循环引用）
let weak = cx.entity().downgrade();
let set = PresenceSet::new(
    cx,
    MotionLifecycle::fade(AnimationSpec::default()),
    move |key, window, cx| {
        if let Some(strong) = weak.upgrade() {
            strong.read(cx).render_item(key, window, cx)
        } else {
            div()
        }
    },
);

// 每帧更新期望状态：true 入场 / false 退场
self.set.update(cx, |s, cx| {
    s.set_present("tab-1", self.tab_1_open, window, cx);
    s.set_present("tab-2", self.tab_2_open, window, cx);
});

// 插入到元素树
row.child(self.set.clone());
```

- **每 key 独立生命周期（S5/S6/S7）**：每个 key 独立沿用 `PresenceState` 的状态机
  语义——S5 epoch 守卫（退场定时器回调仅当 `closing && epoch` 匹配时生效，退场中途
  重开无副作用）、S6 转换快照（进行中的过渡不受后续 `set_lifecycle` 影响）、
  S7 卸载宽限（定时器 = 退场时长 + delay + 50ms，兜底 GPUI 动画起点滞后一帧）。
- **退场完成自动移除**：退场动画结束后条目自动从容器移除（`len()` 减小），
  多 key 互不干扰；`is_present(key)` 返回最近一次声明的期望状态。
- **必须订阅**：`set_present` 内部会 `cx.notify()` —— 调用方需
  `cx.observe(&set, |_, _, cx| cx.notify())` 驱动自身重绘（gallery 演示同款接线）。

### 手势驱动弹簧（DragSpring）

`DragSpring` 是 `SpringValue` 的手势语义封装：不暴露"目标值"，而是暴露拖拽生命周期
（按住 / 拖动 / 松手）。**拖拽期**目标 = 指针位置，弹簧每帧追赶指针产生轻微阻尼的跟手
效果；**松手**目标 = 调用方指定的 settle 点，弹簧从当前值回弹并精确收敛到该点。

```rust
use gpui_component_motion::{DragSpring, SpringPreset};

// 一次创建：初始值 0，Default 预设（轻微阻尼跟手）
let spring = DragSpring::new(cx, 0.0, SpringPreset::Default);

// on_mouse_down：进入跟手模式，弹簧停在当前值
spring.update(cx, |s, cx| s.begin_drag(window, cx));
// on_mouse_move：目标 = 指针位置，弹簧阻尼追赶
spring.update(cx, |s, cx| s.drag_to(pointer_x, window, cx));
// on_mouse_up：目标 = settle 点，松手回弹并精确收敛
spring.update(cx, |s, cx| s.end_drag(settle_x, window, cx));

// render 中读取当前值（拖拽期跟手、松手期回弹）
let current = spring.read(cx).value();
```

- **典型用途**：抽屉拖拽关闭（松手 settle 到关闭位 / 阈值另一侧）、卡片拖走（swipe，
  松手 settle 到屏幕外或回弹原位）、滑块跟手（thumb 位置 = `value()`）。
- **非拖拽态忽略**：未 `begin_drag` 时调用 `drag_to` 直接返回（幂等，不启动任务）。
- **必须订阅**：弹簧每帧 tick 会 `cx.notify()` —— 调用方需
  `cx.observe(&spring, |_, _, cx| cx.notify())` 驱动自身重绘，否则显示值不更新。

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
- **`LoopMotion` 循环同属"永不结束"族（E1/E2）**：`repeat_synced` 循环同样每帧
  tick，常驻挂载会钉住整窗满帧重绘 —— 必须用状态标志条件挂载，卸载即停
  （见「循环动效与数值弹簧（Phase 2）」节与 gallery 演示）。
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
