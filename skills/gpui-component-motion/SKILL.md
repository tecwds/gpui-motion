---
name: "gpui-component-motion"
description: "Guides adding animations to gpui-component UIs via MotionExt, PresenceState/PresenceSet, Spring physics, keyframes, stagger, loops, spring values, drag springs, and MotionTokens presets. Invoke when adding entry/exit animations, panel transitions, list/tab enter-exit, loading loops, animated numbers, draggable elements, or motion effects to GPUI elements."
---

# gpui-component-motion

Non-intrusive animation layer for [gpui-component](https://github.com/longbridge/gpui-component).
Built on top of GPUI's `with_animation` primitive.

Rust edition: **2024**. Crate name: `gpui-component-motion` (**v0.1.1**).

## Core API Map

| Export | Source | Purpose |
|--------|--------|---------|
| `MotionExt` | `src/ext.rs` | Blanket impl trait: `fade_in` / `slide_up` / `slide_down` / `slide_left` / `slide_right` / `with_motion` on any `IntoElement + Styled` |
| `Animated<T>` | `src/animated.rs` | Element wrapper implementing `IntoElement` (and `ParentElement` when `T: ParentElement`); handles enter/exit, Spring mapping, initial-state preset; prebuilds the GPUI `Animation` (per-frame alloc = 1 `Box`, forced by `with_animation` signature) |
| `AnimationSpec` | `src/spec.rs` | Duration + delay + easing + optional Spring; builder methods `with_duration` / `with_delay` / `with_easing` / `with_spring` / `without_spring` |
| `Easing` | `src/easing.rs` | Traditional curves: `Linear` / `EaseIn` / `EaseOut` / `EaseInOut` (output ∈ [0,1]) |
| `SpringPreset` | `src/easing.rs` | Spring physics: `Stiff` / `Default` / `Gentle` / `Wobbly` (output can overshoot >1) |
| `Motion` | `src/motion.rs` | Preset effects: `Fade` / `SlideUp` / `SlideDown` / `SlideLeft` / `SlideRight` / `ExpandWidth` / `ExpandHeight`; Phase 1 adds color interpolation variants `BackgroundColor(from, to)` / `TextColor(from, to)` / `BorderColor(from, to)` (Hsla pairs; entry eases from → to, exit to → from, hue takes the shortest path) |
| `MotionKeyframes<T>` + `Keyframe` | `src/keyframes.rs` | Multi-segment keyframe wrapper (Phase 1 / B4): splits total duration across segments by each `Keyframe`'s relative `ratio` (normalized at build time); builder `keyframe(ratio, easing, motion)`; implements `IntoElement` + `ParentElement`; empty frames fall back to a single Fade |
| `stagger` | `src/stagger.rs` | Free function (Phase 1 / B5): `stagger(Vec<Animated<T>>, gap)` → `Vec<Animated<T>>`, replaces element *i*'s delay with `gap * i` for cascading entry (duration / easing / spring preserved) |
| `MotionLifecycle` | `src/lifecycle.rs` | Enter + exit animation pairing; presets: `fade` / `slide_*` / `expand_width` / `expand_height` |
| `PresenceState` | `src/presence.rs` | Declarative presence container (state machine HIDDEN→ENTERING→VISIBLE→EXITING); auto-manages enter/exit timing and unmount |
| `LoopMotion<T>` + `LoopKind` | `src/loop_motion.rs` | Looping effects (Phase 2 / C7): wraps an `IntoElement + Styled` element in a never-ending animation. `LoopKind::Pulse` (smooth parabola breath, opacity 0.4→1.0→0.4) / `LoopKind::Skeleton` (triangle-wave blink, 0.5→1.0→0.5, shimmer-ish). Convenience ctors `pulse` / `skeleton`; implements `IntoElement` + `ParentElement`; built on `repeat_synced` (shared App clock). **Never ends — must be conditionally mounted and unmounted when done (E1/E2).** |
| `SpringValue` | `src/spring_value.rs` | Declarative numeric spring (Phase 2 / D8): `Entity<SpringValue>`; `new(cx, initial, preset)` → event-driven `set_target(target, window, cx)` → tick-driven interpolation at ~16ms (60fps) with per-frame `notify`; read `value()` / `target()` in render. Curve matches `SpringPreset` exactly (underdamped presets may overshoot; exact convergence at t≥1); mid-flight `set_target` redirects from the current value (no velocity continuity); same-target short-circuits. |
| `PresenceSet` | `src/presence_set.rs` | Multi-key declarative presence container (Phase 3 / E10): `Entity<PresenceSet>`; `new(cx, lifecycle, builder)` — builder is `Fn(&SharedString, &mut Window, &mut App) -> Div` (called every frame per key, should capture `WeakEntity`); event-driven `set_present(key, present, window, cx)` — per-key independent enter/exit with S5 epoch guard / S6 transition snapshot / S7 50ms exit grace, entry auto-removed when exit finishes; `set_lifecycle` / `is_present(key)` / `len()` / `is_empty()`. Notifies internally — owning view must `cx.observe(&set, ...)` to redraw. |
| `DragSpring` | `src/drag.rs` | Gesture-driven spring (Phase 3 / D9): `Entity<DragSpring>`; `new(cx, initial, preset)` → event-driven `begin_drag(window, cx)` / `drag_to(value, window, cx)` / `end_drag(settle, window, cx)`. Drag-phase target = pointer (damped follow); release springs back and converges exactly at `settle`. `value()` / `dragging()`; non-dragging `drag_to` ignored (idempotent). ~16ms tick with per-frame `notify` — owning view must subscribe to redraw. |
| `MotionTokens` | `src/tokens.rs` | Named component motion presets (Phase 4 / F12): `panel` / `tooltip` / `modal` / `notification` / `dropdown` / `toast` — each hardens a "component → enter/exit motion pairing" for zero-config access. `Copy` struct, all fields public (`enter` / `enter_spec` / `exit` / `exit_spec`); ctor `custom(enter, enter_spec, exit, exit_spec)`; methods `animate(inner, id) -> Animated<T>` (one-line entry wrapper), `lifecycle() -> MotionLifecycle` (for `PresenceState` / `PresenceSet`), `with_enter(motion, spec)` / `with_exit(motion, spec)`. **I3 line**: exit specs never carry Spring — presets are built Spring-free and `custom` / `with_exit` force-strip via `AnimationSpec::without_spring()`. |

## Usage Patterns

### 1. Entry animation (one-liner)

```rust
use gpui_component_motion::MotionExt;
use gpui::{div, px, Styled};

div().child(div().fade_in("my-fade"));
div().child(div().slide_up("my-slide", px(10.0)));
```

Customize spec via builder chain on `Animated<T>`:

```rust
div().fade_in("my-fade").with_spec(
    AnimationSpec::default()
        .with_duration(Duration::from_millis(400))
        .with_delay(Duration::from_millis(50))
        .with_easing(Easing::EaseInOut),
);
```

### 2. Spring physics

```rust
div().fade_in("spring-fade").with_spec(
    AnimationSpec::default().with_spring(SpringPreset::Default),
);
```

**How it works**: GPUI internally `debug_assert`s easing output ∈ [0,1], but Spring overshoots >1.
Solution: in Spring mode, GPUI easing is set to `Linear` (passes linear progress through),
and the animator callback maps `t` via `spring.curve(t)` internally. This is already handled
inside `Animated<T>` — callers just set `with_spring`.

| Preset | Damping ζ | Frequency ω₀ | Duration | Overshoot | Use case |
|--------|-----------|---------------|----------|-----------|----------|
| `Stiff` | 1.0 | 35 | 186ms | none | Critical damping, fast (exit, collapse) |
| `Default` | 0.7 | 30 | 219ms | slight | General entry |
| `Gentle` | 0.5 | 25 | 368ms | mild | Large-area transitions |
| `Wobbly` | 0.3 | 28 | 548ms | pronounced | Playful pop effect |

> **Exit animations always strip Spring** (S3): every exit-spec construction path
> (`Animated::new` / `Animated::with_exit` / `MotionLifecycle::new` / `with_exit_spec`)
> removes the Spring preset — overshooting to negative values is meaningless for
> width/opacity. If the duration was not explicitly overridden (still equal to the
> preset's recommended duration), it resets to the default 200ms; explicit durations
> are kept. There is no public path to a Spring exit. The stripping logic lives in the
> public `AnimationSpec::without_spring()`, so it also works outside the crate's
> construction paths (e.g. manually building an `Animated` from stored specs).

> **Terminal style override**: at `t=1` `Motion::apply` applies the motion's terminal
> style, which overrides conflicting existing styles on the element (GPUI `Styled`
> refinement cannot be removed or read back — framework limitation). Wrap the animated
> element instead of animating the element that carries the conflicting style.
> `apply` input clamping (I8): `Fade` / `Expand*` clamp to `[0,1]`, `Slide*` rejects
> negatives but allows overshoot `>1`.

### 3. Declarative Presence (enter + exit)

```rust
use gpui_component_motion::{AnimationSpec, MotionLifecycle, PresenceState, SpringPreset};

// Create once
let presence = PresenceState::new(
    cx,
    "right-panel",
    MotionLifecycle::expand_width(
        px(340.),
        AnimationSpec::default().with_spring(SpringPreset::Default),
    )
    .with_exit_spec(AnimationSpec::default().with_duration(Duration::from_millis(250))),
    move |window, cx| {
        // Rebuilt every frame; capture WeakEntity to read latest state
        div().w(px(340.)).h_full().child("panel content")
    },
);

// Update desired state every frame
presence.update(cx, |s, cx| s.set_present(is_open, window, cx));

// Insert into element tree
h_flex().child(presence.clone());

// Runtime mode switching (e.g. Spring ↔ Easing comparison)
presence.update(cx, |s, cx| s.set_lifecycle(new_lifecycle, cx));
```

State machine: `HIDDEN → ENTERING → VISIBLE → EXITING → HIDDEN`.
Each transition increments `epoch` to ensure unique animation `ElementId`s (avoids GPUI
caching stale `delta=1.0` animation state).

- **Transition snapshots (S6)**: `set_present` captures the current enter/exit motion +
  spec pair into `enter_active` / `exit_active` at the moment of the transition. In-flight
  transitions are NOT affected by later `set_lifecycle` calls — the new lifecycle only
  applies to subsequent transitions. `set_lifecycle` with an equal lifecycle short-circuits
  (no notify, no snapshot touch). Snapshots are cleared on reverse transition or timer
  completion.
- **Exit timer grace (S7)**: the unmount timer is `exit_spec.duration + delay + 50ms`
  (`EXIT_TIMER_GRACE`), covering GPUI's start-timestamp being recorded at first layout
  (up to one frame late).
- **Epoch guard (S5)**: the timer callback only acts when `closing` is still true AND the
  epoch matches the one captured at spawn; stale timers from interrupted exits are silently
  ignored.

### 4. Attaching children (ParentElement, S12)

`Animated<T>` implements `ParentElement` when `T: ParentElement`, so children attach
directly (they are applied to the wrapped element before animation starts):

```rust
div().fade_in("panel").child(div().child("panel content"));
```

### 5. Motion presets

| Motion | Enter | Exit | Use case |
|--------|-------|------|----------|
| `Fade` | opacity 0→1 | opacity 1→0 | General fade |
| `SlideUp(off)` | top: +off→0 | top: 0→+off | Slide in from below |
| `SlideDown(off)` | top: -off→0 | top: 0→-off | Slide in from above |
| `SlideLeft(off)` | left: +off→0 | left: 0→+off | Slide in from right (right panel) |
| `SlideRight(off)` | left: -off→0 | left: 0→-off | Slide in from left (left panel) |
| `ExpandWidth(max)` | width: 0→max | width: max→0 | Horizontal panel expand/collapse |
| `ExpandHeight(max)` | height: 0→max | height: max→0 | Vertical collapse (dropdown/accordion) |
| `BackgroundColor(from, to)` | bg from→to | bg to→from | Accent color gradient, state coloring |
| `TextColor(from, to)` | text from→to | text to→from | Text highlight, state text color |
| `BorderColor(from, to)` | border from→to | border to→from | Selection / validation border highlight |

### 6. Phase 1: composition (color / keyframes / stagger)

Three Phase 1 capabilities compose with everything above. Colors use `MotionExt::with_motion`; keyframes and stagger are new standalone APIs.

**Color interpolation** — color variants take a `(from, to)` `Hsla` pair; the element's color eases from `from` to `to` during entry (hue walks the shortest path):

```rust
use gpui_component_motion::{AnimationSpec, Motion, MotionExt};
use gpui::{div, hsla, px, Styled};
use std::time::Duration;

// bg red → blue over 600ms
div().with_motion(
    "color-bg",
    AnimationSpec::default().with_duration(Duration::from_millis(600)),
    Motion::BackgroundColor(hsla(0.0, 0.9, 0.5, 1.0), hsla(0.6, 0.9, 0.5, 1.0)),
);
```

`TextColor` / `BorderColor` behave identically for `text_color` / `border_color`.

**Keyframes** — `MotionKeyframes` splits the total duration across segments by relative `ratio` weight (normalized at build time; each segment ≥ 1ms, the last segment absorbs rounding). Each segment's `Motion` completes its own from→to within the segment:

```rust
use gpui_component_motion::{Easing, Motion, MotionKeyframes};
use gpui::{div, px, Styled};
use std::time::Duration;

// 600ms total: first half fades in, second half slides up 16px
MotionKeyframes::new(div(), "kf", Duration::from_millis(600))
    .keyframe(1.0, Easing::EaseOut, Motion::Fade)
    .keyframe(1.0, Easing::EaseOut, Motion::SlideUp(px(16.0)));
```

**Stagger** — free function that replaces element *i*'s delay with `gap * i` (delay 0 / gap / 2*gap / …; duration / easing / spring preserved):

```rust
use gpui_component_motion::{MotionExt, stagger};
use gpui::{div, Styled};
use std::time::Duration;

let items = vec![
    div().fade_in("item-0"),
    div().fade_in("item-1"),
    div().fade_in("item-2"),
];
let staggered = stagger(items, Duration::from_millis(80)); // 0 / 80 / 160ms
```

> **Unique ids per item (E3)**: each staggered item is a sibling, so every item needs its own `ElementId`. Use a stable base name per item (e.g. `ElementId::named_usize("item-0", n)`) with the replay counter — never `format!` in render.

### 7. Phase 2: LoopMotion & SpringValue (C7 / D8)

**Loops must be mounted on demand (E1/E2).** `LoopMotion` wraps an element with a
never-ending `repeat_synced` animation — while mounted it ticks every frame and pins the
window to full-rate redraws for the whole session. Mount it only while the effect is
needed (e.g. a `loading` / `pulsing` flag), unmount when done (**unmount = stop**; the
animation state dies with the element), and never render it idle:

```rust
use std::time::Duration;
use gpui_component_motion::{LoopKind, LoopMotion};
use gpui::{div, Styled};

// Mount only while needed; render a static element otherwise
let block = if loading {
    // Pulse: smooth breath, opacity 0.4 → 1.0 → 0.4
    LoopMotion::pulse(
        div().w(px(120.)).h(px(80.)).rounded_lg().bg(accent), // accent = theme accent
        "pulse-card",
        Duration::from_millis(900),
    )
} else {
    div().w(px(120.)).h(px(80.)).rounded_lg().bg(accent)
};
```

`LoopKind::Pulse` is a smooth parabola breath (0.4→1.0→0.4); `LoopKind::Skeleton` is a
triangle-wave blink (0.5→1.0→0.5, shimmer-ish — GPUI has no gradient-position styles
yet). Both modulate the wrapped element's overall opacity (children included). Loops run
on `repeat_synced`, sharing the App clock with all other synced loops (one frame schedule).

**`SpringValue` is a declarative numeric spring.** Create the `Entity` once, declare
targets on events, read the interpolated value in render:

```rust
use gpui_component_motion::{SpringPreset, SpringValue};

// Create once: initial 0, Wobbly preset (pronounced overshoot)
let value = SpringValue::new(cx, 0.0, SpringPreset::Wobbly);

// Event-driven: spring interpolates from the current value to 100
value.update(cx, |s, cx| s.set_target(100.0, window, cx));

// In render (every frame, tick-driven notify):
let current = value.read(cx).value();
```

- Interpolation follows the `SpringPreset` curve exactly (`from + (to-from) * progress(t)`)
  — underdamped presets may overshoot mid-flight; `t ≥ 1` converges exactly (no residual).
- Mid-flight `set_target` redirects from the current value (`from = current`); the old tick
  task is dropped and a generation guard ignores stale ticks. No velocity continuity.
- Same-target calls short-circuit (no new task, no notify).
- Read `value()` / `target()` only in render; never call `set_target` from render.

### 8. Phase 3: PresenceSet & DragSpring (E10 / D9)

**`PresenceSet` is the multi-key generalization of `PresenceState`.** Create the
`Entity<PresenceSet>` once with a shared `MotionLifecycle` and a builder that rebuilds
each key's child every frame; then declare each key's desired presence on events. Every key
gets its own independent enter/exit lifecycle (S5 epoch guard, S6 transition snapshots,
S7 50ms exit grace) — typical for list add/remove, tabs, notification stacks:

```rust
use gpui_component_motion::{AnimationSpec, MotionLifecycle, PresenceSet};

// Create once; the builder is Fn(&SharedString, &mut Window, &mut App) -> Div and
// should capture a WeakEntity (not Entity) to read latest state without cycles.
let set = PresenceSet::new(
    cx,
    MotionLifecycle::fade(AnimationSpec::default()),
    move |key, _window, cx| div().child(key.clone()),
);

// Event-driven: enter / exit per key (entry auto-removed when exit finishes)
set.update(cx, |s, cx| {
    s.set_present("tab-1", self.tab_1_open, window, cx);
    s.set_present("tab-2", self.tab_2_open, window, cx);
});

// Insert as a child element
row.child(set.clone());
```

- Per-key state is fully independent: `is_present(key)` reports the latest declared
  target; `len()` includes entries still playing their exit animation.
- `set_present` / the exit timer both `notify` the container — **the owning view must
  `cx.observe(&set, |_, _, cx| cx.notify())` to redraw** (same wiring as the Phase 2
  `SpringValue` cards in the gallery).

**`DragSpring` is the gesture-semantics wrapper of `SpringValue`.** Instead of declaring
a target, wire the drag lifecycle: during drag the target follows the pointer (the spring
dampens, so the element trails slightly); on release it springs back to the settle point
and converges exactly (no residual):

```rust
use gpui_component_motion::{DragSpring, SpringPreset};

let spring = DragSpring::new(cx, 0.0, SpringPreset::Default);

// on_mouse_down -> begin_drag(window, cx): enter follow mode, spring holds at current value
// on_mouse_move -> drag_to(pointer_x, window, cx): target = pointer, damped chase
// on_mouse_up   -> end_drag(settle_x, window, cx): release, spring back to settle
```

- `drag_to` outside a drag (no `begin_drag`) is ignored (idempotent, no task spawned).
- `value()` renders the current position; `dragging()` distinguishes follow vs. settle.
- The spring ticks at ~16ms and `notify`s every frame — **the owning view must
  `cx.observe(&spring, |_, _, cx| cx.notify())` to redraw**, otherwise the rendered
  position never updates.

### 9. Phase 4: MotionTokens — zero-config presets (F12)

`MotionTokens` hardens common "component → motion pairing" into named presets
(`panel` / `tooltip` / `modal` / `notification` / `dropdown` / `toast`), so callers
never hand-assemble `Motion` + `AnimationSpec` pairs. It is a `Copy` struct with public
fields; customize via `custom(enter, enter_spec, exit, exit_spec)`, or per-side
`with_enter(motion, spec)` / `with_exit(motion, spec)`.

**Consumption 1 — one-line entry (zero-config access):**

```rust
use gpui_component_motion::MotionTokens;
use gpui::{div, px, Styled};

// Plays the token's enter animation at render time — no Motion/AnimationSpec juggling
let el = MotionTokens::panel().animate(
    div().w(px(340.)).h(px(80.)).child("content"),
    "panel-el",
);
```

**Consumption 2 — full lifecycle pairing for presence containers:**

```rust
use gpui_component_motion::MotionTokens;

// Equivalent to hand-building MotionLifecycle::fade(...) / expand_width(...) etc.
let lc = MotionTokens::modal().lifecycle();
let presence = PresenceState::new(cx, "dialog", lc, builder);
// or: let set = PresenceSet::new(cx, MotionTokens::notification().lifecycle(), builder);
```

**I3 line (no-Spring exits):** every token's `exit_spec` never carries Spring —
overshooting to negative values is meaningless for width/opacity. Presets are built
Spring-free; `custom` / `with_exit` force-strip via `AnimationSpec::without_spring()`.
There is no construction path to a Spring exit.

## Critical Constraints

These are hard-won lessons. Violating them causes visual bugs (instant appear/disappear,
layout jumps, stale animation state).

1. **Animation containers must use `flex_shrink_0`** — without it, flex layout compresses
   the animated element during transition, causing visual jitter.

2. **`PresenceState` must increment `epoch` on both entry AND exit** — ensures unique
   animation IDs. GPUI's `with_element_state` caches by `(GlobalElementId, TypeId)`;
   reusing an ID after a completed animation returns cached `delta=1.0` (no animation).

3. **Exit animations must preset `initial_t = 1`** — the first frame of exit must start
   from the fully-visible state (t=1), not from t=0. Otherwise the element flashes to
   its start state before animating out.

4. **Hidden state placeholder must have `w(px(0.0))`, `h_full()`, `flex_shrink_0`** —
   prevents layout collapse/jump when the element is in HIDDEN state.

5. **Do NOT combine `opacity` with position slides** — causes visual artifacts.
   Slide motions should be pure position (top/left) transitions. The `Motion::Slide*`
   variants already omit opacity; do not add `.opacity(t)` manually.

6. **`Animated<T>` requires `T: Styled`** — `AnyElement` does NOT implement `Styled`.
   The PresenceState child builder returns `Div` (not `AnyElement`) for this reason.

7. **Child builder closure must capture `WeakEntity`, not `Entity`** — avoids reference
   cycles. The closure is `Rc<dyn Fn>` (called every frame), not `FnOnce`.

8. **Infinite loops must be conditionally mounted (E1/E2)** — `LoopMotion` (and any
   `repeat_synced` / `repeat()` element) never ends and ticks every frame while mounted,
   pinning the window to full-rate redraws for the whole session. Mount only while the
   effect is needed (state flag), unmount to stop (unmount = stop; state dies with the
   element), and never render it idle.

## Common Pitfalls

### "Animation appears instantly, then plays"

Cause: `Animated<T>` did not preset the initial state (t=0) before the first frame.
Fix: The library handles this internally by calling `motion.apply(self.inner, 0.0)`
before the animation starts. If you see this bug, check that your `Motion::apply`
implementation correctly sets the start state at t=0.

### "Exit animation disappears instantly"

Cause: GPUI's `with_animation` only supports entry. Exit requires a separate mechanism.
Fix: Use `PresenceState` which manages the exit lifecycle, or use `Animated::with_exit()`
to set up the reverse animation. The `PresenceState::set_present` guard must be
`if present == self.target { return; }` to avoid infinite re-render.

### "Panel width jumps instantly then animates"

Cause: Using a small `SlideLeft(40px)` offset when the panel is 340px wide — the layout
change (340px appearing/disappearing) dwarfs the 40px slide.
Fix: Use `Motion::ExpandWidth(340px)` instead — the layout change itself becomes the
animation. Wrap children in `overflow_hidden` to prevent content overflow during collapse.

### "Spring causes GPUI panic (debug_assert delta ∈ [0,1])"

Cause: Spring overshoot produces values >1, triggering GPUI's internal assertion.
Fix: Already handled in `Animated<T>` — Spring mode sets GPUI easing to `Linear` and
maps `t` internally. Do not pass `SpringPreset` directly to GPUI's `Animation` constructor.

### Edition 2024 RPIT lifetime capture

Edition 2024 makes `impl Trait` return types capture all in-scope lifetimes (including
anonymous `&str` parameters). If a function returns `impl IntoElement` and takes `&str`
from a non-`'static` source, the borrow won't live long enough.

Fix: Take owned `String` instead of `&str` for parameters used to construct element IDs.

## Known Limitations

- **Interruption jump (S8)**: GPUI `Animation` does not support custom start progress, so
  interrupting exit→enter / enter→exit causes one visible jump (from the current frame
  position to the new animation's start). No workaround; documented behavior.
- **Expand reflow cost (S10)**: `ExpandWidth` / `ExpandHeight` change size every frame,
  triggering taffy subtree reflow and text relayout — avoid on large text subtrees.
- **Delay redraw cost (S11)**: `delay` is folded into the easing prefix; GPUI's
  `AnimationElement` requests a frame every tick until `done`, so the delay period still
  redraws at full rate. Budget for long delays (>300ms).
- **Zero-duration clamp (S1)**: `duration + delay == 0` is clamped to a minimum 1ms
  animation (`MIN_ANIMATION_DURATION`); no NaN/div-by-zero path. Pure-delay specs output
  the start state during the delay period and the terminal state at `t=1`.
- **Blanket impl exclusivity**: `MotionExt` is a blanket impl — downstream crates cannot
  implement it for their own types and the method names (`fade_in`, `slide_up`, …) are
  globally claimed. Intentional design (one-line integration).

## Examples

```sh
cargo run --example gallery   # Component animation showcase (main demo)
cargo run --example codex     # Codex desktop UI clone (3-column + Presence + Spring/Easing toggle)
cargo run --example story     # Animation storyboard (all Motion × Easing combos)
```

## Dev Commands

```sh
just check     # cargo check --all-targets
just test      # cargo test
just fmt       # cargo fmt
just lint      # cargo clippy --all-targets -- -D warnings
just ci        # fmt-check + clippy + test
just run       # cargo run --example gallery
```
