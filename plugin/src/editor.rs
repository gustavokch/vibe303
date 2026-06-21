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
#[cfg(feature = "adaptive-os")]
use crate::{Oversampling, WdfTier};

// ── Palette ─────────────────────────────────────────────────────────────────────────────
// Warm-tinted neutrals (no pure black/white). amber = value/focus; oxblood = active/pressed.
const PANEL: Color32 = Color32::from_rgb(0x1E, 0x1C, 0x1A);
const RAISED: Color32 = Color32::from_rgb(0x26, 0x23, 0x20);
const BORDER: Color32 = Color32::from_rgb(0x3A, 0x36, 0x33);
const TEXT: Color32 = Color32::from_rgb(0xE8, 0xE2, 0xD8);
const DIM: Color32 = Color32::from_rgb(0x8A, 0x82, 0x7A);
const AMBER: Color32 = Color32::from_rgb(0xD9, 0x8A, 0x3D);
const OXBLOOD: Color32 = Color32::from_rgb(0x9E, 0x3B, 0x2E);

// ── Spacing scale (px) ──────────────────────────────────────────────────────────────────
// Every `add_space` in the layout draws from this set — no magic numbers. Tight tokens group
// related controls; larger tokens break between stages. The full-height column rules carry the
// major left→right separation, so the vertical scale stays fine-grained inside each column.
const SP_XS: f32 = 6.0; // tightest: intra-group row gaps
const SP_SM: f32 = 8.0; // cozy: between sibling controls
const SP_RULE: f32 = 10.0; // breathing room around an inline rule
const SP_MD: f32 = 12.0; // snug: frame margins, label-to-control
const SP_LG: f32 = 16.0; // section-internal: between control rows
const SP_XL: f32 = 22.0; // major: between logical sections within a column

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

/// Default editor canvas size, in logical points (egui zoom scales the whole canvas uniformly,
/// so these are the layout budget regardless of host DPI). Single source of truth: `lib.rs`
/// passes these to `EguiState::from_size`, and `columns_fit_window` (tests) asserts the
/// signal-flow columns fit inside `EDITOR_W`. Change the width here and the test catches drift.
pub(crate) const EDITOR_W: u32 = 1040;
pub(crate) const EDITOR_H: u32 = 660;

/// Fixed logical widths of the four signal-flow columns: INPUT · EQUALIZER · MODEL · OUTPUT.
/// Hoisted to a const so the layout and the fit-test (`columns_fit_window`) share one budget.
const COL_W: [f32; 4] = [92.0, 360.0, 320.0, 140.0];

fn draw_ui(ctx: &egui::Context, setter: &ParamSetter, p: &Arc<Ba283Params>) {
    // Variant 0 = NeuralNet, 1 = Wdf. Avoid importing the enum: read the normalized value.
    let wdf_on = p.backend.modulated_normalized_value() > 0.5;

    egui::CentralPanel::default().show(ctx, |ui| {
        // levelNorm ON = neutral auto-staging: the manual INPUT/OUTPUT/LINK controls are forced
        // neutral (the DSP ignores them) and grayed. OFF = faithful manual staging.
        let staging_on = !p.level_norm.value();

        // LINK (levelNorm OFF only): slave OUTPUT to the opposite of INPUT (+X in ⇒ −X out) so the
        // user pushes the circuit hot while the net output level stays put. Driven here from the
        // editor so automation/host see consistent values; the DSP just reads both params. The OUTPUT
        // knob is disabled below while linked, so this never fights a user drag. Runs every frame
        // regardless of layout.
        if staging_on && p.link.value() {
            let desired = (-p.input_gain.value()).clamp(-24.0, 12.0);
            if (p.output_db.value() - desired).abs() > 0.01 {
                setter.begin_set_parameter(&p.output_db);
                setter.set_parameter(&p.output_db, desired);
                setter.end_set_parameter(&p.output_db);
            }
        }

        // ── Top bar: wordmark + backend + global staging modes ────────────────────────
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("BA283")
                    .font(display_font(28.0))
                    .color(TEXT)
                    .extra_letter_spacing(2.0),
            );
            ui.add_space(SP_SM);
            ui.label(
                RichText::new("preamp")
                    .font(display_font(14.0))
                    .color(DIM)
                    .extra_letter_spacing(3.0),
            );
            // GOTCHA: this `right_to_left` MUST stay wrapped in `ui.horizontal` — placed directly in
            // the top-down panel it claims the full height to vertically-centre and shoves the
            // columns off the bottom. LEVEL NORM / LINK live here (a global bar), not under a knob,
            // because they gate controls in two different columns (INPUT col 1, OUTPUT col 4).
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(staging_on, |ui| {
                    toggle_pill(ui, setter, &p.link, "LINK");
                });
                ui.add_space(SP_SM);
                toggle_pill(ui, setter, &p.level_norm, "LEVEL NORM");
                ui.add_space(SP_LG);
                segmented(ui, setter, &p.backend);
            });
        });

        ui.add_space(SP_XS);
        hairline(ui);
        ui.add_space(SP_LG);

        // ── Signal-flow columns, left→right: INPUT · EQUALIZER · MODEL · OUTPUT. Each column is a
        //    fixed-width top-down cell; the `ui.separator()` between them renders a full-height
        //    vertical rule (bounded by this outer horizontal, so NOT the right_to_left trap). ──
        // Wrapped in a `ScrollArea` so the layout is structurally clip-proof: the default
        // `1040×660` window fits everything (no scrollbars appear), but a future control row, a
        // longer localized label, or a host that renders the hole slightly smaller scrolls instead
        // of clipping off-screen — the failure mode this layout originally hit. `auto_shrink=false`
        // keeps the body filling the viewport so the full-height column separators still span it.
        let scroll = egui::ScrollArea::both().auto_shrink([false, false]);
        scroll.show(ui, |ui| {
            ui.horizontal(|ui| {
                // ── Col 1 — INPUT stage ──
                sized_column(ui, COL_W[0], |ui| {
                    labelled_rule(ui, "INPUT");
                    ui.add_space(SP_MD);
                    ui.add_enabled_ui(staging_on, |ui| {
                        knob_cell(ui, setter, &p.input_gain, "INPUT", 60.0);
                    });
                });
                ui.separator();

                // ── Col 2 — EQUALIZER (Neve 1073/1076): backend-agnostic, ahead of the model ──
                // Width is set by the 6-option MID frequency selector (the widest row); it also gives
                // the EQ on/off pill enough room beside the gain knobs so its label never wraps.
                sized_column(ui, COL_W[1], |ui| {
                    labelled_rule(ui, "EQUALIZER");
                    ui.add_space(SP_SM);
                    ui.horizontal(|ui| {
                        knob_cell(ui, setter, &p.eq_low_gain, "LOW", 60.0);
                        ui.add_space(SP_XS);
                        knob_cell(ui, setter, &p.eq_mid_gain, "MID", 60.0);
                        ui.add_space(SP_XS);
                        knob_cell(ui, setter, &p.eq_high_gain, "HIGH", 60.0);
                        ui.add_space(SP_MD);
                        ui.vertical(|ui| {
                            ui.add_space(SP_XL);
                            toggle_pill(ui, setter, &p.eq_on, "EQ");
                        });
                    });
                    ui.add_space(SP_SM);
                    // Switched-frequency selectors stacked one-per-row: keeping HPF + LOW on a single
                    // row makes that row ~530px wide and balloons the whole column. Stacked, the column
                    // width is set by the 6-option MID row (~340px) and the dead space disappears.
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("HPF").font(display_font(12.0)).color(DIM));
                        ui.add_space(SP_XS);
                        segmented(ui, setter, &p.eq_hpf_freq);
                    });
                    ui.add_space(SP_XS);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("LOW").font(display_font(12.0)).color(DIM));
                        ui.add_space(SP_XS);
                        segmented(ui, setter, &p.eq_low_freq);
                    });
                    ui.add_space(SP_XS);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("MID").font(display_font(12.0)).color(DIM));
                        ui.add_space(SP_XS);
                        segmented(ui, setter, &p.eq_mid_freq);
                    });
                });
                ui.separator();

                // ── Col 3 — MODEL / CIRCUIT (the heart): circuit_gain · DRIVE · BIAS · AGE · TRANSFORMER
                //    · quality. circuit_gain + DRIVE are backend-agnostic (stay live for NN); the rest is
                //    WDF-only and fades/disables under the Neural Net backend. ──
                sized_column(ui, COL_W[2], |ui| {
                    // Header is "MODEL" (not "MODEL (WDF)"): the column leads with circuit_gain + DRIVE,
                    // which are backend-agnostic and stay live under the Neural Net backend. The WDF-only
                    // scope is conveyed by the faded/disabled sub-frame and its "active in WDF mode" hint.
                    labelled_rule(ui, "MODEL");
                    ui.add_space(SP_SM);
                    // Always-live row (full-opacity under NN): circuit_gain self-scopes past the frame
                    // fade; DRIVE is applied in both backends.
                    ui.horizontal(|ui| {
                        circuit_gain_cell(ui, setter, p);
                        ui.add_space(SP_SM);
                        knob_cell(ui, setter, &p.drive_db, "DRIVE", 60.0);
                    });
                    ui.add_space(SP_MD);

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
                                // BIAS · AGE · RE-ROLL — WDF-only character controls.
                                ui.horizontal(|ui| {
                                    knob_cell(ui, setter, &p.bias, "BIAS", 64.0);
                                    ui.add_space(SP_SM);
                                    knob_cell(ui, setter, &p.age, "AGE", 64.0);
                                    ui.add_space(SP_RULE);
                                    ui.vertical(|ui| {
                                        ui.add_space(SP_XL);
                                        reroll_button(ui, setter, p);
                                        ui.add_space(SP_XS);
                                        let seed = p.age_seed.load(Ordering::Relaxed);
                                        ui.label(
                                            RichText::new(format!("UNIT {:04X}", seed & 0xFFFF))
                                                .font(value_font(12.0))
                                                .color(DIM),
                                        );
                                    });
                                });

                                ui.add_space(SP_MD);
                                hairline(ui);
                                ui.add_space(SP_MD);

                                // ── TRANSFORMER (Jiles-Atherton core): pill on its own line above the
                                //    two dials so it never crowds them off the column's right edge. ──
                                #[cfg(feature = "trafo-core")]
                                {
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.add_space(SP_XL);
                                            toggle_pill(ui, setter, &p.trafo_on, "TRANSFORMER");
                                        });
                                        ui.add_space(SP_MD);
                                        knob_cell(ui, setter, &p.trafo_hyst, "HYSTERESIS", 64.0);
                                        ui.add_space(SP_SM);
                                        knob_cell(ui, setter, &p.trafo_sat, "SATURATION", 64.0);
                                    });
                                    ui.add_space(SP_MD);
                                    hairline(ui);
                                    ui.add_space(SP_MD);
                                }

                                // Quality selectors — stacked rows (each fits the column width; the
                                // original single wide row would overflow a 300px column).
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("TIER").font(display_font(12.0)).color(DIM),
                                    );
                                    ui.add_space(SP_XS);
                                    segmented(ui, setter, &p.wdf_tier);
                                });
                                ui.add_space(SP_XS);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("PRECISION")
                                            .font(display_font(12.0))
                                            .color(DIM),
                                    );
                                    ui.add_space(SP_XS);
                                    segmented(ui, setter, &p.precision);
                                });
                                ui.add_space(SP_XS);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("OVERSAMPLING")
                                            .font(display_font(12.0))
                                            .color(DIM),
                                    );
                                    ui.add_space(SP_XS);
                                    segmented(ui, setter, &p.oversampling);
                                    // Adaptive level-gated oversampling: only meaningful on the lossy
                                    // tiers (UltraFast/Warp) under Oversampling = Auto. Greyed elsewhere.
                                    #[cfg(feature = "adaptive-os")]
                                    {
                                        let adaptive_scope = matches!(
                                            p.wdf_tier.value(),
                                            WdfTier::UltraFast | WdfTier::Warp
                                        ) && p.oversampling.value()
                                            == Oversampling::Auto;
                                        ui.add_space(SP_LG);
                                        ui.add_enabled_ui(adaptive_scope, |ui| {
                                            toggle_pill(ui, setter, &p.adaptive_os, "ADAPTIVE OS");
                                        });
                                    }
                                });
                            });
                        });

                    if !wdf_on {
                        ui.add_space(SP_XS);
                        ui.label(
                            RichText::new("Circuit controls active in WDF mode")
                                .font(display_font(11.0))
                                .color(DIM)
                                .extra_letter_spacing(1.0),
                        );
                    }
                });
                ui.separator();

                // ── Col 4 — OUTPUT stage: DPR (output-path phase rotator) → OUTPUT gain → MIX ──
                sized_column(ui, COL_W[3], |ui| {
                    labelled_rule(ui, "OUTPUT");
                    ui.add_space(SP_MD);
                    // DPR (Dual Phase Rotator) — all-pass phase rotator after the model + levelNorm,
                    // before the output gain; reshapes waveform asymmetry without touching magnitude.
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.add_space(SP_XL);
                            toggle_pill(ui, setter, &p.dpr_on, "DPR");
                        });
                        ui.add_space(SP_SM);
                        ui.add_enabled_ui(p.dpr_on.value(), |ui| {
                            knob_cell(ui, setter, &p.dpr_amount, "AMOUNT", 60.0);
                        });
                    });
                    ui.add_space(SP_MD);
                    // OUTPUT is live under levelNorm (post-normalization trim); when levelNorm is OFF
                    // it only grays while LINK slaves it. (`!staging_on` ⇒ levelNorm ON ⇒ always live.)
                    ui.add_enabled_ui(!staging_on || !p.link.value(), |ui| {
                        knob_cell(ui, setter, &p.output_db, "OUTPUT", 60.0);
                    });
                    ui.add_space(SP_SM);
                    knob_cell(ui, setter, &p.mix, "MIX", 60.0);
                });
            });
        });
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

/// Lay `add` into a fixed-width, top-down column. Used by the signal-flow column row so each
/// stage pins to its own width — a bare `ui.vertical` sizes to content and the columns drift
/// ragged. Height claims the panel's remaining height so the inter-column `ui.separator()` rules
/// span the full body. This full-height claim is bounded by the outer `ui.horizontal`, so unlike a
/// bare `right_to_left` it does NOT shove content off the bottom.
///
/// INVARIANT: this relies on being called as a direct child of a `ui.horizontal` — there every
/// sibling sees the same `available_height` (a horizontal layout consumes width, not height), so
/// all columns claim equal full height and the inter-column separators line up. Nest `sized_column`
/// inside a vertical/top-down layout and that no longer holds — the separators would ragged.
fn sized_column(ui: &mut egui::Ui, w: f32, add: impl FnOnce(&mut egui::Ui)) {
    let h = ui.available_height();
    ui.allocate_ui_with_layout(
        egui::vec2(w, h),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(w);
            add(ui);
        },
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

/// The CIRCUIT GAIN cell (stepped or continuous). Backend-AGNOSTIC: it sets the WDF R_KJ leg AND
/// conditions the NeuralNet (FiLM gain-norm), so it stays live + full-opacity even when the rest of
/// the WDF-only panel is greyed for NN — a nested scope overrides the frame's `wdf_on` disable +
/// opacity fade.
fn circuit_gain_cell(ui: &mut egui::Ui, setter: &ParamSetter, p: &Arc<Ba283Params>) {
    ui.scope(|ui| {
        ui.set_opacity(1.0);
        ui.add_enabled_ui(true, |ui| {
            // The hardware gain switch is an IntParam(0..4): each index is a detent (normalized pos +
            // dB label). The knob hard-steps between them — a faithful rotary, never continuous.
            let detents: Vec<(f32, &'static str)> = crate::GAIN_STEPS
                .iter()
                .enumerate()
                .map(|(i, s)| (p.circuit_gain.preview_normalized(i as i32), s.label))
                .collect();
            knob_cell_stepped(ui, setter, &p.circuit_gain, "CIRCUIT GAIN", 64.0, &detents);
        });
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

    /// Guards the hardcoded width budget: the four signal-flow columns + their separators + the
    /// item-spacing gaps must fit inside the editor canvas. Catches drift when a column width grows
    /// or the window shrinks (the silent right-clip failure mode). Generous estimates for the
    /// non-column costs so the test only fires on real overflow, never on egui spacing tweaks.
    #[test]
    fn columns_fit_window() {
        const SEP_W: f32 = 6.0; // egui Separator main-axis size
        const GAP: f32 = 8.0; // s.spacing.item_spacing.x set in apply_theme
        const PANEL_MARGIN: f32 = 16.0; // CentralPanel inner margin, both sides combined
        let cols: f32 = COL_W.iter().sum();
        // Layout: col sep col sep col sep col → 3 separators, 6 inter-widget gaps.
        let budget = cols + 3.0 * SEP_W + 6.0 * GAP + PANEL_MARGIN;
        assert!(
            budget <= EDITOR_W as f32,
            "signal-flow columns need {budget}px but the window is only {EDITOR_W}px wide",
        );
    }
}
