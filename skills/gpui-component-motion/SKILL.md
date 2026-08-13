---
name: "gpui-component-motion"
description: "Guides adding animations to gpui-component UIs via MotionExt, PresenceState, and Spring physics. Invoke when adding entry/exit animations, panel transitions, or motion effects to GPUI elements."
---

# gpui-component-motion

Non-intrusive animation layer for [gpui-component](https://github.com/longbridge/gpui-component).
Built on top of GPUI's `with_animation` primitive.

Rust edition: **2024**. Crate name: `gpui-component-motion`.

## Core API Map

| Export | Source | Purpose |
|--------|--------|---------|
| `MotionExt` | `src/ext.rs` | Blanket impl trait: `fade_in` / `slide_up` / `slide_down` / `slide_left` / `slide_right` / `with_motion` on any `IntoElement + Styled` |
| `Animated<T>` | `src/animated.rs` | Element wrapper implementing `IntoElement`; handles enter/exit, Spring mapping, initial-state preset |
| `AnimationSpec` | `src/spec.rs` | Duration + delay + easing + optional Spring; builder methods `with_duration` / `with_delay` / `with_easing` / `with_spring` |
| `Easing` | `src/easing.rs` | Traditional curves: `Linear` / `EaseIn` / `EaseOut` / `EaseInOut` (output ∈ [0,1]) |
| `SpringPreset` | `src/easing.rs` | Spring physics: `Stiff` / `Default` / `Gentle` / `Wobbly` (output can overshoot >1) |
| `Motion` | `src/motion.rs` | Preset effects: `Fade` / `SlideUp` / `SlideDown` / `SlideLeft` / `SlideRight` / `ExpandWidth` / `ExpandHeight` |
| `MotionLifecycle` | `src/lifecycle.rs` | Enter + exit animation pairing; presets: `fade` / `slide_*` / `expand_width` / `expand_height` |
| `PresenceState` | `src/presence.rs` | Declarative presence container (state machine HIDDEN→ENTERING→VISIBLE→EXITING); auto-manages enter/exit timing and unmount |

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
> are kept. There is no public path to a Spring exit.

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

### 4. Motion presets

| Motion | Enter | Exit | Use case |
|--------|-------|------|----------|
| `Fade` | opacity 0→1 | opacity 1→0 | General fade |
| `SlideUp(off)` | top: +off→0 | top: 0→+off | Slide in from below |
| `SlideDown(off)` | top: -off→0 | top: 0→-off | Slide in from above |
| `SlideLeft(off)` | left: +off→0 | left: 0→+off | Slide in from right (right panel) |
| `SlideRight(off)` | left: -off→0 | left: 0→-off | Slide in from left (left panel) |
| `ExpandWidth(max)` | width: 0→max | width: max→0 | Horizontal panel expand/collapse |
| `ExpandHeight(max)` | height: 0→max | height: max→0 | Vertical collapse (dropdown/accordion) |

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
