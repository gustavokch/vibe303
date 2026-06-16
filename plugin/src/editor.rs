//! Custom egui editor for the BA283 — a compact, single-window "modern vintage-console"
//! GUI. Warm-charcoal panel, amber value arcs, oxblood active accent, condensed silkscreen
//! labels. Pure presentation: it only reads/writes parameters the host already owns (via
//! `ParamSetter`) — no DSP, no audio-thread work. The whole CIRCUIT sub-panel dims + disables
//! when the Neural Net backend is active, since `wdf_tier`/`precision`/`circuit_gain`/`age`
//! are inert there.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, Align2, Color32, FontFamily, FontId, RichText, Sense, Stroke};
use nih_plug_egui::{create_egui_editor, EguiState};

use crate::{Ba283Params, UiScale};

// ── Palette ─────────────────────────────────────────────────────────────────────────────
// Warm-tinted neutrals (no pure black/white). amber = value/focus; oxblood = active/pressed.
const PANEL: Color32 = Color32::from_rgb(0x1E, 0x1C, 0x1A);
const RAISED: Color32 = Color32::from_rgb(0x26, 0x23, 0x20);
const BORDER: Color32 = Color32::from_rgb(0x3A, 0x36, 0x33);
const TEXT: Color32 = Color32::from_rgb(0xE8, 0xE2, 0xD8);
const DIM: Color32 = Color32::from_rgb(0x8A, 0x82, 0x7A);
const AMBER: Color32 = Color32::from_rgb(0xD9, 0x8A, 0x3D);
const OXBLOOD: Color32 = Color32::from_rgb(0x9E, 0x3B, 0x2E);

/// Condensed silkscreen family (labels / wordmark).
fn display_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("display".into()))
}
/// Numeric readout family (Archivo; proportional — readout width is not fixed-pitch).
fn value_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("value".into()))
}

/// The DPI scale the host reported via `IPlugViewContentScaleSupport::set_scale_factor`, stored
/// as raw `f32` bits (1.0 = unset / no host scaling). Shared between [`ScaleAwareEditor`] (which
/// captures it) and the per-frame update closure (which applies it as the egui zoom). See
/// [`ScaleAwareEditor`] for why we apply it through egui's zoom instead of letting baseview do it.
type HostScale = Arc<AtomicU32>;

/// Resolve the egui zoom factor from the host-reported scale and the user's `UI Scale` override.
/// `Auto` follows the host (the common, correct path); the explicit factors are an escape hatch
/// for hosts that never report a scale, or that report the wrong one.
///
/// Linux/X11 only — see the call site for why this is gated to one platform.
#[cfg(any(target_os = "linux", test))]
fn effective_zoom(host_scale: f32, manual: UiScale) -> f32 {
    match manual {
        // Follow the host, but clamp: a garbage value (0, NaN, huge) before the first real
        // callback would otherwise zoom the UI to an unusable size the user can't click out of
        // to reach this override. 0.5..4.0 spans every real HiDPI ratio.
        UiScale::Auto => {
            if host_scale.is_finite() && host_scale > 0.0 {
                host_scale.clamp(0.5, 4.0)
            } else {
                1.0
            }
        }
        UiScale::X100 => 1.0,
        UiScale::X125 => 1.25,
        UiScale::X150 => 1.5,
        UiScale::X175 => 1.75,
        UiScale::X200 => 2.0,
    }
}

/// Build the editor. `params` is read/written through the passed `ParamSetter` each frame;
/// `editor_state` carries the persisted window size.
pub(crate) fn create(
    params: Arc<Ba283Params>,
    editor_state: Arc<EguiState>,
) -> Option<Box<dyn Editor>> {
    // Captured from the host in `ScaleAwareEditor::set_scale_factor`, read each frame below.
    let host_scale: HostScale = Arc::new(AtomicU32::new(1.0f32.to_bits()));
    // Only the Linux path reads this in-closure; cloning it elsewhere is dead weight.
    #[cfg(target_os = "linux")]
    let frame_scale = host_scale.clone();
    let frame_params = params.clone();

    let editor = create_egui_editor(
        editor_state,
        (),
        // build — runs once: install fonts + theme.
        |ctx, _state| apply_theme(ctx),
        // update — runs each frame.
        move |ctx, setter, _state| {
            // Apply HiDPI scaling through egui's own zoom — Linux/X11 ONLY. There baseview ignores
            // both `SystemScaleFactor` and an explicit `WindowScalePolicy::ScaleFactor`, so egui
            // always renders at ppp=1 into a physically larger compositor surface — the UI ends up
            // tiny in the top-left corner. egui's `zoom_factor` is the one lever that *does* take
            // effect: it drives ppp, which baseview honors for the framebuffer/window size, so the
            // UI fills the host's hole. Set before any UI is laid out this frame (avoids a
            // mid-pipeline ppp change), and only when it actually changes (avoids needless relayout).
            //
            // macOS/Windows are deliberately excluded: there baseview/the OS already render at the
            // backing scale, and the host's `set_scale_factor` is forwarded to the inner editor
            // (see `ScaleAwareEditor::set_scale_factor`) which handles DPI natively. Adding egui
            // zoom on top of that would double-scale. This mirrors the macOS exclusion the previous
            // `set_scale_factor(1.0)` pin carried, now widened to Windows for the same reason.
            #[cfg(target_os = "linux")]
            {
                let host = f32::from_bits(frame_scale.load(Ordering::Relaxed));
                let zoom = effective_zoom(host, frame_params.ui_scale.value());
                if (ctx.zoom_factor() - zoom).abs() > 1e-3 {
                    ctx.set_zoom_factor(zoom);
                }
            }
            draw_ui(ctx, setter, &frame_params);
        },
    )?;

    Some(Box::new(ScaleAwareEditor { inner: editor, host_scale }))
}

/// Wraps the `nih_plug_egui` editor to capture the host's DPI scale.
///
/// The host calls `set_scale_factor` (via VST3 `IPlugViewContentScaleSupport`) before the window
/// opens, and the VST3 wrapper multiplies the *reported* editor size by it to size the host's
/// window "hole". But baseview does not actually render at that scale on Linux/X11, so without
/// intervention the egui content (ppp=1) is far smaller than the hole. We capture the value here
/// and re-apply it as egui's `zoom_factor` in the update closure, so the rendered framebuffer
/// matches the hole exactly. All other `Editor` methods delegate unchanged.
struct ScaleAwareEditor {
    inner: Box<dyn Editor>,
    host_scale: HostScale,
}

impl Editor for ScaleAwareEditor {
    fn spawn(
        &self,
        parent: ParentWindowHandle,
        context: Arc<dyn GuiContext>,
    ) -> Box<dyn std::any::Any + Send> {
        self.inner.spawn(parent, context)
    }

    fn size(&self) -> (u32, u32) {
        self.inner.size()
    }

    fn set_scale_factor(&self, factor: f32) -> bool {
        // Capture for the egui zoom regardless of whether the inner editor accepts it (it rejects
        // changes while open); the wrapper still uses our reported size × factor for the hole.
        self.host_scale.store(factor.to_bits(), Ordering::Relaxed);
        self.inner.set_scale_factor(factor)
    }

    fn param_value_changed(&self, id: &str, normalized_value: f32) {
        self.inner.param_value_changed(id, normalized_value);
    }

    fn param_modulation_changed(&self, id: &str, modulation_offset: f32) {
        self.inner.param_modulation_changed(id, modulation_offset);
    }

    fn param_values_changed(&self) {
        self.inner.param_values_changed();
    }
}

// ── Theme ───────────────────────────────────────────────────────────────────────────────

fn apply_theme(ctx: &egui::Context) {
    // Bundled OFL fonts (see assets/fonts/LICENSE).
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "barlow".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/BarlowSemiCondensed-SemiBold.ttf"
        ))),
    );
    // Archivo is a variable font; its fvar default is wght=600 (SemiBold), wdth=100 (normal), so
    // ab_glyph instances the readouts at SemiBold — the intended weight, no axis selection needed.
    fonts.font_data.insert(
        "archivo".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/Archivo-Variable.ttf"
        ))),
    );
    // Each named family carries a second face as a glyph fallback so a character missing from the
    // primary (e.g. a future unit like Ω/±/µ) renders from the other bundled face instead of tofu.
    fonts.families.insert(
        FontFamily::Name("display".into()),
        vec!["barlow".to_owned(), "archivo".to_owned()],
    );
    fonts.families.insert(
        FontFamily::Name("value".into()),
        vec!["archivo".to_owned(), "barlow".to_owned()],
    );
    // Use Archivo as the default proportional face too (host tooltips, fallbacks).
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "archivo".to_owned());
    ctx.set_fonts(fonts);

    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = RAISED;
    v.faint_bg_color = RAISED;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = OXBLOOD;
    v.selection.stroke = Stroke::new(1.0, AMBER);
    v.hyperlink_color = AMBER;

    v.widgets.noninteractive.bg_fill = RAISED;
    v.widgets.noninteractive.weak_bg_fill = RAISED;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, DIM);

    v.widgets.inactive.bg_fill = RAISED;
    v.widgets.inactive.weak_bg_fill = RAISED;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);

    v.widgets.hovered.bg_fill = BORDER;
    v.widgets.hovered.weak_bg_fill = BORDER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, AMBER);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);

    v.widgets.active.bg_fill = OXBLOOD;
    v.widgets.active.weak_bg_fill = OXBLOOD;
    v.widgets.active.bg_stroke = Stroke::new(1.0, AMBER);
    v.widgets.active.fg_stroke = Stroke::new(1.0, TEXT);

    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 8.0);
        s.spacing.button_padding = egui::vec2(9.0, 4.0);
    });
}

// ── Layout ──────────────────────────────────────────────────────────────────────────────

fn draw_ui(ctx: &egui::Context, setter: &ParamSetter, p: &Arc<Ba283Params>) {
    // Variant 0 = NeuralNet, 1 = Wdf. Avoid importing the enum: read the normalized value.
    let wdf_on = p.backend.modulated_normalized_value() > 0.5;

    egui::CentralPanel::default().show(ctx, |ui| {
        // ── Header: wordmark + backend toggle ─────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("BA283")
                    .font(display_font(28.0))
                    .color(TEXT)
                    .extra_letter_spacing(2.0),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new("preamp")
                    .font(display_font(14.0))
                    .color(DIM)
                    .extra_letter_spacing(3.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                segmented(ui, setter, &p.backend);
            });
        });

        ui.add_space(6.0);
        hairline(ui);
        ui.add_space(18.0);

        // ── Core knob row ─────────────────────────────────────────────────────────────
        ui.columns(4, |cols| {
            knob(&mut cols[0], setter, &p.drive_db, "DRIVE", 66.0);
            knob(&mut cols[1], setter, &p.bias, "BIAS", 66.0);
            knob(&mut cols[2], setter, &p.output_db, "OUTPUT", 66.0);
            knob(&mut cols[3], setter, &p.mix, "MIX", 66.0);
        });

        ui.add_space(10.0);

        // Level-norm pill, right-aligned under the knob row. The `right_to_left` layout MUST be
        // nested in a `ui.horizontal` (as the header is): placed directly in the top-down panel
        // it claims the full remaining height to vertically-center, pushing the CIRCUIT panel off
        // the bottom of the viewport. The horizontal wrapper bounds the row to its content height.
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                toggle_pill(ui, setter, &p.level_norm, "LEVEL NORM");
            });
        });

        ui.add_space(16.0);

        // ── OUTPUT SATURATION (ADAA2) — backend-agnostic output stage, so it lives OUTSIDE the
        //    WDF-only CIRCUIT panel. AMOUNT knob + ON/OFF pill. ──
        labelled_rule(ui, "OUTPUT SATURATION");
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            knob_cell(ui, setter, &p.sat_amount, "AMOUNT", 64.0);
            ui.add_space(10.0);
            ui.vertical(|ui| {
                ui.add_space(22.0);
                toggle_pill(ui, setter, &p.sat_on, "SATURATION");
            });
        });

        ui.add_space(16.0);

        // ── CIRCUIT (WDF) sub-panel ───────────────────────────────────────────────────
        labelled_rule(ui, "CIRCUIT (WDF)");
        ui.add_space(10.0);

        // Cross-fade + disable when the Neural Net backend is active.
        let t = ctx.animate_bool_with_time(egui::Id::new("wdf_fade"), wdf_on, 0.15);
        let opacity = 0.4 + 0.6 * t;

        egui::Frame::NONE
            .fill(RAISED)
            .stroke(Stroke::new(1.0, BORDER))
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_opacity(opacity);
                ui.add_enabled_ui(wdf_on, |ui| {
                    ui.horizontal(|ui| {
                        // Use `knob_cell` (NOT bare `knob`) here: `knob` centers via
                        // `vertical_centered`, which claims the full available width. That is
                        // bounded inside the core row's `ui.columns`, but in this free
                        // `ui.horizontal` the first knob would eat all the width and shove the
                        // rest off the right edge. `knob_cell` pins each to its own cell width.
                        if p.gain_stepped.value() {
                            // Map each hardware position to a knob detent (normalized pos + label).
                            let detents: Vec<(f32, &'static str)> = crate::GAIN_STEPS
                                .iter()
                                .map(|s| (p.circuit_gain.preview_normalized(s.knob_ohm), s.label))
                                .collect();
                            knob_cell_stepped(
                                ui,
                                setter,
                                &p.circuit_gain,
                                "CIRCUIT GAIN",
                                64.0,
                                &detents,
                            );
                        } else {
                            knob_cell(ui, setter, &p.circuit_gain, "CIRCUIT GAIN", 64.0);
                        }
                        ui.add_space(8.0);
                        knob_cell(ui, setter, &p.age, "AGE", 64.0);
                        ui.add_space(10.0);
                        ui.vertical(|ui| {
                            ui.add_space(22.0);
                            reroll_button(ui, setter, p);
                            ui.add_space(6.0);
                            let seed = p.age_seed.load(Ordering::Relaxed);
                            ui.label(
                                RichText::new(format!("UNIT {:04X}", seed & 0xFFFF))
                                    .font(value_font(12.0))
                                    .color(DIM),
                            );
                            ui.add_space(8.0);
                            toggle_pill(ui, setter, &p.gain_stepped, "STEPPED");
                        });
                    });

                    ui.add_space(12.0);
                    hairline(ui);
                    ui.add_space(12.0);

                    // ── TRANSFORMER (Jiles-Atherton core) — its own row so the pill + two dials
                    //    never crowd the gain/age row off the right edge on HiDPI. ──
                    #[cfg(feature = "trafo-core")]
                    {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.add_space(22.0);
                                toggle_pill(ui, setter, &p.trafo_on, "TRANSFORMER");
                            });
                            ui.add_space(12.0);
                            knob_cell(ui, setter, &p.trafo_hyst, "HYSTERESIS", 64.0);
                            ui.add_space(8.0);
                            knob_cell(ui, setter, &p.trafo_sat, "SATURATION", 64.0);
                        });
                        ui.add_space(12.0);
                        hairline(ui);
                        ui.add_space(12.0);
                    }

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("TIER").font(display_font(12.0)).color(DIM));
                        ui.add_space(6.0);
                        segmented(ui, setter, &p.wdf_tier);
                        ui.add_space(20.0);
                        ui.label(RichText::new("PRECISION").font(display_font(12.0)).color(DIM));
                        ui.add_space(6.0);
                        segmented(ui, setter, &p.precision);
                        ui.add_space(20.0);
                        ui.label(RichText::new("OVERSAMPLING").font(display_font(12.0)).color(DIM));
                        ui.add_space(6.0);
                        segmented(ui, setter, &p.oversampling);
                    });
                });
            });

        if !wdf_on {
            ui.add_space(6.0);
            ui.label(
                RichText::new("Circuit controls active in WDF mode")
                    .font(display_font(11.0))
                    .color(DIM)
                    .extra_letter_spacing(1.0),
            );
        }
    });
}

/// A faint full-width hairline separator.
fn hairline(ui: &mut egui::Ui) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 1.0), Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0, BORDER),
    );
}

/// A section rule with a left-aligned condensed caps label sitting on the line.
fn labelled_rule(ui: &mut egui::Ui, label: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .font(display_font(12.0))
                .color(DIM)
                .extra_letter_spacing(2.0),
        );
        let w = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 1.0), Sense::hover());
        ui.painter().line_segment(
            [rect.left_center(), rect.right_center()],
            Stroke::new(1.0, BORDER),
        );
    });
}

// ── Custom arc-knob widget ──────────────────────────────────────────────────────────────

/// Per-knob scroll-gesture state held in egui temp memory: whether a host parameter gesture is
/// currently open and the time of the last scroll tick (for idle-close).
#[derive(Clone, Copy, Default)]
struct ScrollGesture {
    active: bool,
    last: f64,
}

/// The bounding-box a knob (arc + label) occupies, derived from its arc diameter. Shared by
/// `knob` (the region it allocates) and `knob_cell` (the cell it pins to) so the two cannot drift.
fn knob_cell_size(diameter: f32) -> egui::Vec2 {
    egui::vec2(diameter + 20.0, diameter + 36.0)
}

/// `knob` wrapped in a fixed-width cell so it is safe inside a free `ui.horizontal` row. `knob`
/// itself centers via `vertical_centered`, which grabs the full available width — fine inside a
/// fixed-width `ui.columns` column, but it spreads/overflows in an unbounded horizontal layout.
/// Bounding the region to the knob's own cell width keeps the centered look and advances the
/// cursor by exactly one cell.
fn knob_cell<P: Param>(ui: &mut egui::Ui, setter: &ParamSetter, param: &P, label: &str, diameter: f32) {
    let cell = knob_cell_size(diameter);
    ui.allocate_ui_with_layout(cell, egui::Layout::top_down(egui::Align::Center), |ui| {
        knob(ui, setter, param, label, diameter);
    });
}

/// A 270° arc knob driven by any `Param`. Honors the param's own normalized↔plain mapping
/// (so the dB ranges and `circuit_gain`'s skew come through for free) and formats the
/// readout with the param's own formatter. Drag (vertical) / scroll to adjust, Shift = fine,
/// double-click resets to default. Greys out + ignores input when `ui.is_enabled()` is false.
fn knob<P: Param>(ui: &mut egui::Ui, setter: &ParamSetter, param: &P, label: &str, diameter: f32) {
    knob_impl(ui, setter, param, label, diameter, None);
}

/// `knob_cell` for a hardware-stepped knob: snaps drag/scroll to the supplied detent positions
/// (normalized value + label) and shows the step label instead of the param formatter. Used by
/// the stepped `circuit_gain` knob; every other knob uses the plain `knob`/`knob_cell` path.
fn knob_cell_stepped<P: Param>(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    param: &P,
    label: &str,
    diameter: f32,
    detents: &[(f32, &'static str)],
) {
    let cell = knob_cell_size(diameter);
    ui.allocate_ui_with_layout(cell, egui::Layout::top_down(egui::Align::Center), |ui| {
        knob_impl(ui, setter, param, label, diameter, Some(detents));
    });
}

/// Backing implementation for [`knob`]. `detents`, when `Some`, makes the knob hard-step: input is
/// snapped to the nearest detent's normalized position and the readout shows the detent label.
fn knob_impl<P: Param>(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    param: &P,
    label: &str,
    diameter: f32,
    detents: Option<&[(f32, &'static str)]>,
) {
    let enabled = ui.is_enabled();
    let cell = knob_cell_size(diameter);
    // Snap a normalized value to the nearest detent position (identity when continuous).
    let snap = |n: f32| -> f32 {
        match detents {
            Some(ds) if !ds.is_empty() => {
                let mut best = n;
                let mut best_d = f32::INFINITY;
                for &(dn, _) in ds {
                    let d = (dn - n).abs();
                    if d < best_d {
                        best_d = d;
                        best = dn;
                    }
                }
                best
            }
            _ => n,
        }
    };

    ui.vertical_centered(|ui| {
        let (rect, mut response) = ui.allocate_exact_size(
            cell,
            if enabled {
                Sense::click_and_drag()
            } else {
                Sense::hover()
            },
        );

        let mut norm = param.modulated_normalized_value();

        if enabled {
            let fine = ui.input(|i| i.modifiers.shift);
            let scale = if fine { 0.25 } else { 1.0 };

            if response.double_clicked() {
                let d = snap(param.default_normalized_value());
                set_norm(setter, param, d);
                norm = d;
                response.mark_changed();
            } else {
                if response.drag_started() {
                    setter.begin_set_parameter(param);
                }
                if response.dragged() {
                    let dn = -response.drag_delta().y / 200.0 * scale;
                    if dn != 0.0 {
                        norm = snap((norm + dn).clamp(0.0, 1.0));
                        setter.set_parameter_normalized(param, norm);
                    }
                }
                if response.drag_stopped() {
                    setter.end_set_parameter(param);
                }
                // Scroll-to-adjust, bundled into ONE host gesture per burst. A naive
                // begin/set/end per scroll tick logs a separate one-shot automation
                // gesture for every notch; instead open the gesture on the first tick
                // and close it after a short idle (a trailing repaint delivers the
                // closing frame). State lives in egui temp memory keyed by the knob.
                let scroll = if response.hovered() {
                    ui.input(|i| i.smooth_scroll_delta.y)
                } else {
                    0.0
                };
                let now = ui.input(|i| i.time);
                let gid = ui.make_persistent_id(("ba283_knob_scroll", label));
                let mut g: ScrollGesture =
                    ui.memory(|m| m.data.get_temp(gid)).unwrap_or_default();
                if scroll != 0.0 {
                    if !g.active {
                        setter.begin_set_parameter(param);
                        g.active = true;
                    }
                    g.last = now;
                    let dn = scroll / 600.0 * scale;
                    norm = snap((norm + dn).clamp(0.0, 1.0));
                    setter.set_parameter_normalized(param, norm);
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(180));
                } else if g.active && now - g.last > 0.15 {
                    setter.end_set_parameter(param);
                    g.active = false;
                }
                ui.memory_mut(|m| m.data.insert_temp(gid, g));
            }
        }

        // ── Paint ─────────────────────────────────────────────────────────────────────
        let dim = if enabled { 1.0 } else { 0.35 };
        let center = egui::pos2(rect.center().x, rect.top() + diameter * 0.5 + 2.0);
        let radius = diameter * 0.5;
        let painter = ui.painter_at(rect);

        // Dial face.
        painter.circle_filled(center, radius * 0.66, RAISED.gamma_multiply(dim));
        painter.circle_stroke(center, radius * 0.66, Stroke::new(1.0, BORDER.gamma_multiply(dim)));

        // Track arc + value arc. 270° sweep, gap at the bottom.
        let a0 = (-135.0f32).to_radians();
        let a1 = (135.0f32).to_radians();
        let av = a0 + norm * (a1 - a0);
        painter.add(egui::Shape::line(
            arc_points(center, radius, a0, a1, 48),
            Stroke::new(3.0, BORDER.gamma_multiply(dim)),
        ));
        painter.add(egui::Shape::line(
            arc_points(center, radius, a0, av, 48),
            Stroke::new(3.0, AMBER.gamma_multiply(dim)),
        ));

        // Pointer.
        let dir = egui::vec2(av.sin(), -av.cos());
        painter.line_segment(
            [center + dir * (radius * 0.34), center + dir * (radius * 0.62)],
            Stroke::new(2.5, TEXT.gamma_multiply(dim)),
        );

        // Centered value readout. Stepped knobs show the detent label; continuous knobs use the
        // param's own formatter.
        let readout = match detents {
            Some(ds) if !ds.is_empty() => {
                let mut best = ds[0].1;
                let mut best_d = f32::INFINITY;
                for &(dn, lbl) in ds {
                    let d = (dn - norm).abs();
                    if d < best_d {
                        best_d = d;
                        best = lbl;
                    }
                }
                best.to_string()
            }
            _ => param.normalized_value_to_string(norm, true),
        };
        painter.text(
            center,
            Align2::CENTER_CENTER,
            readout,
            value_font(13.0),
            TEXT.gamma_multiply(dim),
        );

        // Label below the dial.
        painter.text(
            egui::pos2(rect.center().x, rect.bottom() - 8.0),
            Align2::CENTER_CENTER,
            label,
            display_font(12.0),
            DIM.gamma_multiply(dim),
        );
    });
}

/// Sample a stroked arc as a polyline between two angles (radians, measured clockwise from
/// 12 o'clock — matches the knob's `dir = (sin, -cos)` convention).
fn arc_points(center: egui::Pos2, radius: f32, a0: f32, a1: f32, segments: usize) -> Vec<egui::Pos2> {
    (0..=segments)
        .map(|i| {
            let a = a0 + (a1 - a0) * (i as f32 / segments as f32);
            center + egui::vec2(a.sin(), -a.cos()) * radius
        })
        .collect()
}

/// One-shot normalized write wrapped in a begin/end gesture (for discrete edits: double-click,
/// scroll tick, segmented click).
fn set_norm<P: Param>(setter: &ParamSetter, param: &P, norm: f32) {
    setter.begin_set_parameter(param);
    setter.set_parameter_normalized(param, norm);
    setter.end_set_parameter(param);
}

// ── Segmented selector (enums + bools) ──────────────────────────────────────────────────

/// A row of pill buttons, one per discrete step of the param. Works for any `Param` whose
/// `step_count()` is finite (all `EnumParam`s and `BoolParam`s). Uses the param's own
/// per-step label via `normalized_value_to_string`.
fn segmented<P: Param>(ui: &mut egui::Ui, setter: &ParamSetter, param: &P) {
    let Some(steps) = param.step_count() else {
        return;
    };
    let n = steps + 1;
    let cur = param.modulated_normalized_value();
    let cur_idx = (cur * steps as f32).round() as usize;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for i in 0..n {
            let norm = if steps == 0 { 0.0 } else { i as f32 / steps as f32 };
            let label = param.normalized_value_to_string(norm, false).to_uppercase();
            let selected = i == cur_idx;
            let resp = ui.add(egui::SelectableLabel::new(
                selected,
                RichText::new(label).font(display_font(12.0)).color(if selected {
                    TEXT
                } else {
                    DIM
                }),
            ));
            if resp.clicked() {
                set_norm(setter, param, norm);
            }
        }
    });
}

/// Single pill that reflects + toggles a `BoolParam` (the Level Norm control).
fn toggle_pill(ui: &mut egui::Ui, setter: &ParamSetter, param: &BoolParam, label: &str) {
    let on = param.modulated_normalized_value() > 0.5;
    let resp = ui.add(egui::SelectableLabel::new(
        on,
        RichText::new(label)
            .font(display_font(12.0))
            .color(if on { TEXT } else { DIM })
            .extra_letter_spacing(1.0),
    ));
    if resp.clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, !on);
        setter.end_set_parameter(param);
    }
}

/// Momentary "re-roll unit" button. Drives `age_reroll` from the button's held state so the
/// audio thread's rising-edge detector (`last_reroll` in `process`) sees a clean low→high→low
/// pulse spanning ≥1 block — no DSP change needed. The edge is only consumed while `process`
/// runs, so a re-roll pressed with the transport stopped takes effect once audio resumes.
fn reroll_button(ui: &mut egui::Ui, setter: &ParamSetter, p: &Arc<Ba283Params>) {
    let resp = ui.add(
        egui::Button::new(RichText::new("RE-ROLL").font(display_font(12.0)).color(TEXT))
            .fill(RAISED)
            .stroke(Stroke::new(1.0, BORDER)),
    );
    let held = resp.is_pointer_button_down_on();
    let cur = p.age_reroll.modulated_normalized_value() > 0.5;
    if held != cur {
        setter.begin_set_parameter(&p.age_reroll);
        setter.set_parameter(&p.age_reroll, held);
        setter.end_set_parameter(&p.age_reroll);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_follows_finite_host_scale() {
        assert_eq!(effective_zoom(1.0, UiScale::Auto), 1.0);
        assert_eq!(effective_zoom(2.0, UiScale::Auto), 2.0);
        assert_eq!(effective_zoom(1.25, UiScale::Auto), 1.25);
    }

    #[test]
    fn auto_guards_garbage_host_scale() {
        // Before the first real callback, or a misreporting host.
        assert_eq!(effective_zoom(0.0, UiScale::Auto), 1.0);
        assert_eq!(effective_zoom(-1.0, UiScale::Auto), 1.0);
        assert_eq!(effective_zoom(f32::NAN, UiScale::Auto), 1.0);
        assert_eq!(effective_zoom(f32::INFINITY, UiScale::Auto), 1.0);
    }

    #[test]
    fn auto_clamps_to_usable_range() {
        // Too small / too large get pulled into 0.5..4.0 so the UI stays reachable.
        assert_eq!(effective_zoom(0.1, UiScale::Auto), 0.5);
        assert_eq!(effective_zoom(99.0, UiScale::Auto), 4.0);
    }

    #[test]
    fn explicit_overrides_ignore_host_scale() {
        // A garbage host scale must not leak through a manual override.
        assert_eq!(effective_zoom(99.0, UiScale::X100), 1.0);
        assert_eq!(effective_zoom(99.0, UiScale::X125), 1.25);
        assert_eq!(effective_zoom(99.0, UiScale::X150), 1.5);
        assert_eq!(effective_zoom(99.0, UiScale::X175), 1.75);
        assert_eq!(effective_zoom(99.0, UiScale::X200), 2.0);
    }
}
