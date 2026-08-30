# GUI Audit — Round 9 (2026-08-29)

Scope: `plugin303/src/gui/**` rendered at the shipped window size (1366 x 960 logical,
2732 x 1984 physical capture). Evidence: fresh captures in `target/gui-qa/` (default, fun,
running states) plus headless `egui::Context` measurements of the real panel functions.

## Verified fixed since round 8

The round-8 HIGH (bottom two panels missing right margin and border) is resolved.
`crop_right_margin.png` and `crop_bottom_margin.png` show a border and an outer margin on
every panel, including the generator panel (MUTATE) and the transport column.

## Findings

### HIGH-1 — Header right control group is not right-aligned (424 px dead gap)

`plugin303/src/gui/panels/header_bar.rs:93-135`

The TOPOLOGY / QUALITY / ZOOM cluster is meant to sit flush against the right edge of the
header bar. It does not. Measured extents at 100 % zoom:

| Item | Expected | Actual |
|---|---|---|
| right-group container `max_rect` | 865 – 1345 | 865 – 1345 (correct) |
| ZOOM group right edge | 1345 | 922 |
| rightmost header pixel (screenshot) | ~1345 | 916 |

Root cause is an egui layout rule, not the rect maths: the first child of the
`Layout::right_to_left` scope is `ui.vertical(|ui| { ... ZOOM ... })`. A nested `vertical`
inside a right-to-left layout takes the full remaining width and lays its content out from
the **left** edge, so the ZOOM block anchors at x = 866 and every later widget flows
leftwards from there. Isolated repro in a headless context:

```
ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
    ui.vertical(|ui| { ui.label("ZOOMPROBE"); });  // rect = 866 .. 944   (left edge!)
    ui.label("SECOND");                            // rect = 809 .. 858
});
// a bare ui.label() as first item lands at 1268 .. 1346 (flush right, as intended)
```

Fix: give the ZOOM block an explicit box instead of a bare `vertical`, e.g.
`ui.allocate_ui_with_layout(vec2(54.0, 34.0), Layout::top_down(Align::Min), |ui| { ... })`,
or build it with `ui.add_sized`. The unit tests for `right_group_left` all pass, because the
defect is downstream of that function — add a rendered-extent assertion instead.

### HIGH-2 — Zoom presets above 100 % clip the UI with no way to reach the hidden controls

`plugin303/src/gui/panels/header_bar.rs:104-135`, `plugin303/src/lib.rs:77`,
`plugin303/src/gui/mod.rs:34-40`

The editor window is fixed (`EguiState::from_size(1366, 960)`, no resize request), the
layout has no `ScrollArea`, and the header offers 125 %, 150 % and 200 % zoom presets.
Headless measurement of the full panel stack:

| Zoom | Logical viewport | Content min_rect |
|---|---|---|
| 100 % | 1366 x 960 | 1356 x 963 |
| 150 % | 911 x 640 | 1356 x 979 |
| 200 % | 683 x 480 | 1356 x 954 |

At 150 % roughly a third of the surface is unreachable; at 200 % about half is, including
the whole transport column and the second sequencer row. Pick one: drop the presets above
100 %, wrap the central panel body in a `ScrollArea`, or resize the window through
`GuiContext::request_resize` when the zoom changes.

### HIGH-3 — Muted text fails WCAG AA on every background

`plugin303/src/gui/theme.rs` (`TEXT_MUTED = #6e6e80`, 49 usages)

| Background | Contrast | AA (4.5:1) |
|---|---|---|
| CHASSIS_BG #131315 | 3.72 | fail |
| PANEL_BG #1a1a1e | 3.47 | fail |
| PANEL_SOUPED #18181c | 3.54 | fail |
| WELL_BG #161619 | 3.62 | fail |
| SURFACE_ELEVATED #222228 | 3.17 | fail |

Every use is at 7.5–10 px, so the large-text 3:1 exemption does not apply. Affected copy
includes the JITTER label, the CALIBRATED status line, inactive slide-switch options and
all disabled-state values. Raising the token to about #8f8fa4 reaches 4.6:1 on PANEL_BG and
keeps the tonal intent. Everything else in the palette passes: TEXT_SECONDARY 10.0,
ACCENT_ORANGE 6.6, ACCENT_RUBY 5.0 on PANEL_BG.

### MEDIUM-4 — Knob value badge re-implements every parameter's unit mapping

`plugin303/src/gui/widgets/knob.rs` (value-string block)

The badge text is produced by a ~20-branch `self.label.contains("…")` chain that converts
the normalized value with its own hand-written maths, rather than asking the parameter
(`param.to_string()` / `normalized_value_to_string`). CUTOFF, for example, recomputes
`300.0 * 50.0_f32.powf(t)` locally; if the parameter's range or skew is ever retuned, the
badge silently reports a different number than the host and the DSP. The dispatch is also
name-based and order-dependent: "TUNING" contains the substring "IN" and is only saved from
the IN/OUT/TILT branch by branch ordering, and renaming any knob label silently changes its
displayed unit. Replace the chain with the parameter's own formatter.

### MEDIUM-5 — Large voids and inconsistent alignment in four panels

Measured horizontal gaps between content clusters (logical px, from `latest.png`):

- ANALOG CHARACTER & TWEAKS: 357, 170, 306 px voids. The four columns carry no headings
  (unlike the generator panel directly below, whose columns are numbered and titled) and
  each aligns differently: column 1 top-left, column 2 a single centred knob, column 3
  label-left plus centred knob, column 4 a centred button.
- SOUPED-UP ENGINE: nine 24 px knobs spread over 1200 px with 113–128 px between clusters.
- ENVELOPES frame: ~386 px void between the MEG ADSR and VEG ADSR groups, which sit pinned
  to opposite ends of the frame.
- TRANSPORT & PATTERN column: ~180 px void between the BPM row and the bottom
  rate/mode/CLEAR row.

None of this is a rendering bug, but at this density the panels read as unfinished rather
than as deliberate breathing room.

### MEDIUM-6 — No accessibility semantics on any custom widget

No `Response::widget_info` call exists anywhere in `plugin303/src/gui`, so AccessKit
exposes none of the knobs, faders, slide switches, step buttons or LEDs to assistive
technology. There is also no keyboard path to any parameter (no focus handling, no
arrow-key nudge) — the only key handled in the whole GUI is Escape in the piano-roll popup
(`widgets/piano_roll_popup.rs:134`). Knobs additionally ignore the scroll wheel, which is a
host-standard gesture in plugin UIs.

### MEDIUM-7 — Per-frame allocations in a continuously repainting editor

The editor redraws at display rate, so these run 60 times per second:

- `gui/mod.rs:33` — `egui_ctx.set_visuals(Visuals::dark())` clones the entire `Style` every
  frame. Set it once in the build callback.
- `gui/mod.rs` — `send_viewport_cmd(ViewportCommand::Title(...))` is issued every frame.
- `widgets/step_button.rs` — `ui.make_persistent_id(format!("step_drag_{}", step_idx))`
  allocates a `String` per step per frame (32 per frame at 2 x 16 steps). Use
  `Id::new(("step_drag", step_idx))`.
- `widgets/knob.rs` — each knob emits ~48 individual `line_segment` shapes for the track and
  value arcs, about 1200 shapes per frame across the 25 knobs. One polyline per arc would
  collapse this to 2 shapes per knob.

### LOW-8 — Type scale bottoms out at 7.5–8.5 px

Of 19 explicit `FontId` sizes, 10 are 8.5 px and one is 7.5 px (the ADSR value row). On a
HiDPI host this is legible; on a 1x host it renders at 8 physical pixels.

### LOW-9 — Sub-44 px hit targets in the step grid

ACC / SLD pills are ~36 x 24 px and the octave arrows ~36 x 28 px. Acceptable for a mouse,
tight for a trackpad; worth widening if the grid ever gains a compact mode.

### LOW-10 — Combobox and button fills diverge

`16th`, `Direct`, `TOL` and the seed field use the egui default combobox fill, which is
visibly lighter than the adjacent custom buttons (`CLEAR`, `NEW SEED`). Controls of equal
weight read as two different families. Style `visuals.widgets` once, in the same place the
theme is applied.

### LOW-11 — No vertical headroom left

Headless measurement puts the content stack at 963.5 logical px against the fixed 960 px
window; the shipped capture still shows a 17 px bottom gap, so nothing is clipped today.
The margin is thin enough that a different font fallback on a host could clip the last row.

### LOW-12 — `unwrap()` on the shared generator state in the render closure

`gui/mod.rs:42` — `gen_params_state.lock().unwrap()` panics the editor thread if the mutex
is ever poisoned. Prefer recovering from the poison and rendering the last-known state.

## Working-tree note

`plugin303/src/gui/panels/tweaks_panel.rs` carries an uncommitted separator-width correction
(3 x (6 + 2 x 8) = 66 px overhead, column width 317 px), and `plugin303/tests/gui_layout_test.rs`
is still untracked. Both should land before the next round so the audit baseline matches HEAD.
