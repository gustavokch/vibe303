use nih_plug::prelude::{FloatParam, ParamSetter};
use nih_plug_egui::egui::{
    pos2, vec2, Align, CornerRadius, FontId, Frame, Layout, Margin, Rect, Stroke, Ui,
};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, MiniFader, SlideSwitch};
use crate::params::Plugin303Params;

/// Envelope Generators:
/// - MEG ADSR: Modulation Envelope Generator (modulates VCF cutoff filter modulation)
/// - VEG ADSR: Voice / Volume Envelope Generator (modulates VCA amplitude)
#[allow(clippy::too_many_arguments)]
fn render_adsr_bank(
    ui: &mut Ui,
    title: &str,
    attack: &FloatParam,
    decay: &FloatParam,
    sustain: &FloatParam,
    release: &FloatParam,
    setter: &ParamSetter,
    is_active: bool,
) {
    let accent = if is_active {
        theme::ACCENT_ORANGE
    } else {
        theme::TEXT_MUTED
    };

    let bank_frame = Frame::new()
        .fill(theme::WELL_BG)
        .stroke(Stroke::new(1.0_f32, theme::BORDER_DEFAULT))
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::symmetric(4, 2));

    bank_frame.show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        ui.vertical(|ui| {
            // Title. Both states share one allocation path keyed off the
            // cached galley size, so the frame height is identical whether
            // the bank is active or not (round-17 issue 1: 2 px shift came
            // from ui.label padding in the active branch vs a fixed 11.0 px
            // allocation in the inactive branch). When the bank is inactive
            // the parent Ui is disabled, so paint directly on a fade-cleared
            // painter clone: ui.label would get egui's fade-to-background on
            // top of the muted token (round-13 issue 6).
            let title_color = if is_active {
                theme::TEXT_PRIMARY
            } else {
                theme::TEXT_SECONDARY
            };
            let title_font = FontId::new(8.5, nih_plug_egui::egui::FontFamily::Proportional);
            // Allocate the measured title width only — `available_width`
            // here is the whole frame row and would stretch the bank.
            // The galley is cached in egui temp data so neither state
            // allocates a String + galley every frame (PR#61 review;
            // round-10 MEDIUM-7). `title_color` must be part of the key:
            // `layout_no_wrap` bakes the color into the galley mesh, so a
            // bank toggling active↔inactive would otherwise repaint the
            // previous state's color (PR#64 review). Color has no metrics
            // impact — frame height stays identical in both states.
            let galley_id = ui.id().with(("adsr_bank_title", title, title_color));
            let galley = ui
                .ctx()
                .data_mut(|d| d.get_temp::<std::sync::Arc<nih_plug_egui::egui::Galley>>(galley_id));
            let galley = match galley {
                Some(g) => g,
                None => {
                    let g = ui.fonts(|f| {
                        f.layout_no_wrap(title.to_string(), title_font.clone(), title_color)
                    });
                    ui.ctx().data_mut(|d| d.insert_temp(galley_id, g.clone()));
                    g
                }
            };
            let (title_rect, _) = ui.allocate_exact_size(
                vec2(galley.size().x.ceil(), galley.size().y.ceil()),
                nih_plug_egui::egui::Sense::hover(),
            );
            if is_active {
                ui.painter()
                    .galley(title_rect.left_top(), galley, title_color);
            } else {
                let mut p = ui.painter().clone();
                p.set_fade_to_color(None);
                p.galley(title_rect.left_top(), galley, title_color);
            }
            ui.add_space(1.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
                ui.add(MiniFader::new(attack, setter, "A", 40.0, accent));
                ui.add(MiniFader::new(decay, setter, "D", 40.0, accent));
                ui.add(MiniFader::new(sustain, setter, "S", 40.0, accent));
                ui.add(MiniFader::new(release, setter, "R", 40.0, accent));
            });
        });
    });
}

/// Width reserved at the right of the top-controls row for the ENVELOPES
/// module, its separator and the spaces around it. The module itself hugs its
/// content (~251 logical); the remainder is the separator run.
pub(crate) const ENV_GROUP_W: f32 = 290.0;

/// Minimum width the WAVE switch, its divider and the 7-knob strip need before
/// the ENVELOPES group may take any more room.
pub(crate) const LEFT_MIN_W: f32 = 460.0;

/// Left edge of the ENVELOPES group relative to the row origin. Clamped to
/// `left_min` so a narrow row (75 % zoom) never pushes the group over the WAVE
/// switch and the knob strip. Same shape as `header_bar::right_group_left`
/// (round-10 item 1).
pub(crate) fn env_group_left(row_width: f32, left_min: f32, desired: f32) -> f32 {
    (row_width - desired).max(left_min)
}

pub fn render_top_controls(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    // 125 logical: the ENVELOPES module renders ~125 (title row + two ADSR
    // banks of 41-px faders) and must fit inside this panel with symmetric
    // padding instead of overflowing it (round-11 item 2; round-19 review
    // issue 6 follow-up: the old 103 let the module paint ~24 logical below
    // the frame, swallowing the PANEL_GAP gutter).
    let desired_size = vec2(ui.available_width(), 125.0);
    let (rect, _) = ui.allocate_exact_size(desired_size, nih_plug_egui::egui::Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(3), theme::PANEL_BG);
        painter.rect_stroke(
            rect,
            CornerRadius::same(3),
            theme::border_stroke(),
            nih_plug_egui::egui::StrokeKind::Inside,
        );
    }

    // All panel content lives inside one child ui bounded by the exact-size
    // rect: the ENVELOPES module's own allocation can no longer advance the
    // parent cursor past the frame bottom, which used to swallow the
    // PANEL_GAP gutter below (round-19 review, issue 6 follow-up).
    ui.allocate_new_ui(nih_plug_egui::egui::UiBuilder::new().max_rect(rect), |ui| {
        // Zero the child's automatic spacing: allocations below advance the
        // cursor by their exact size only.
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let inner = rect.shrink(6.0);
        // The ENVELOPES module is anchored to the row's right edge and the knob
        // strip expands into everything left of it, so no dead run remains beside
        // the module (round-14 issue 1).
        let env_left = inner.left() + env_group_left(inner.width(), LEFT_MIN_W, ENV_GROUP_W);
        let left_rect = Rect::from_min_max(inner.min, pos2(env_left, inner.bottom()));
        let env_rect = Rect::from_min_max(pos2(env_left, inner.top()), inner.max);

        ui.allocate_new_ui(
            nih_plug_egui::egui::UiBuilder::new().max_rect(left_rect),
            |ui| {
                ui.horizontal(|ui| {
                    // Waveform Selector integrated inside main panel
                    ui.add_space(4.0);
                    ui.add(
                        SlideSwitch::new(&params.waveform, setter, "WAVE", &["SAW", "SQR", "SWP"])
                            .vertical()
                            .with_thumb_color(theme::ACCENT_ORANGE),
                    );
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);

                    // Classic 303 Parameter Gauges: 7 equal columns filling the
                    // strip, the same distribution the souped knob row uses, so the
                    // width freed by the right-anchored ENVELOPES module is spent
                    // on knob spacing instead of dead space (round-14 issue 1).
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    let col_w = ui.available_width() / 7.0;
                    let col_h = ui.available_height();
                    let mut render_knob = |knob: Knob303<'_, _>| {
                        ui.allocate_ui_with_layout(
                            vec2(col_w, col_h),
                            Layout::top_down(Align::Center),
                            |ui| {
                                ui.add(knob);
                            },
                        );
                    };
                    render_knob(Knob303::new(
                        &params.tuning,
                        setter,
                        "TUNING",
                        theme::KNOB_RADIUS_LARGE,
                    ));
                    render_knob(Knob303::new(
                        &params.cutoff,
                        setter,
                        "CUTOFF",
                        theme::KNOB_RADIUS_LARGE,
                    ));
                    render_knob(Knob303::new(
                        &params.resonance,
                        setter,
                        "RESONANCE",
                        theme::KNOB_RADIUS_LARGE,
                    ));
                    render_knob(Knob303::new(
                        &params.env_mod,
                        setter,
                        "ENV MOD",
                        theme::KNOB_RADIUS_LARGE,
                    ));
                    render_knob(Knob303::new(
                        &params.decay,
                        setter,
                        "DECAY",
                        theme::KNOB_RADIUS_LARGE,
                    ));
                    render_knob(Knob303::new(
                        &params.accent,
                        setter,
                        "ACCENT",
                        theme::KNOB_RADIUS_LARGE,
                    ));
                    render_knob(Knob303::new(
                        &params.volume,
                        setter,
                        "VOLUME",
                        theme::KNOB_RADIUS_LARGE,
                    ));
                });
            },
        );

        ui.allocate_new_ui(
            nih_plug_egui::egui::UiBuilder::new().max_rect(env_rect),
            |ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // Envelopes Module Frame: MEG/VEG/ADV ENV distributed cleanly
                    // without isolated controls or dead gaps.
                    let env_frame = Frame::new()
                        .fill(theme::WELL_BG)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER_DEFAULT))
                        .corner_radius(CornerRadius::same(3))
                        .inner_margin(Margin::symmetric(6, 4));

                    env_frame.show(ui, |ui| {
                        // Compact envelope module: hugs its content (title row and
                        // the two ADSR banks) instead of stretching across the
                        // remaining panel width — kills the ~1150 px dead run
                        // between MEG and VEG (round-11 item 3).
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.spacing_mut().interact_size.y = 14.0;
                        ui.vertical(|ui| {
                            let mut is_adv = params.advanced_env.value();
                            ui.horizontal(|ui| {
                                ui.label(
                                    nih_plug_egui::egui::RichText::new("ENVELOPES")
                                        .font(FontId::new(
                                            9.0,
                                            nih_plug_egui::egui::FontFamily::Proportional,
                                        ))
                                        .strong()
                                        // Shared accent title colour (round-11 item 8).
                                        .color(theme::ACCENT_ORANGE),
                                );

                                ui.add_space(10.0);

                                if crate::gui::widgets::checkbox::checkbox(
                                    ui,
                                    &mut is_adv,
                                    "ADV ENV",
                                )
                                .changed()
                                {
                                    setter.begin_set_parameter(&params.advanced_env);
                                    setter.set_parameter(&params.advanced_env, is_adv);
                                    setter.end_set_parameter(&params.advanced_env);
                                }
                            });

                            ui.add_space(2.0);

                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);

                                // One dimming mechanism only (round-13 issue 6):
                                // add_enabled_ui blocks interaction, while
                                // render_adsr_bank/MiniFader paint the disabled bank
                                // with the explicit muted palette on a painter whose
                                // fade-to-background is cleared — egui's disabled
                                // fade must not compound on top of the muted tokens.
                                ui.add_enabled_ui(is_adv, |ui| {
                                    render_adsr_bank(
                                        ui,
                                        "MEG ADSR (Mod Envelope)",
                                        &params.meg_attack,
                                        &params.meg_decay_adj,
                                        &params.meg_sustain,
                                        &params.meg_release,
                                        setter,
                                        is_adv,
                                    );
                                });

                                ui.add_space(8.0);

                                // VEG (voice envelope) is live whether or not ADV
                                // ENV is engaged, so it must render enabled — the
                                // disabled tier is reserved for MEG (round-11 item 7).
                                render_adsr_bank(
                                    ui,
                                    "VEG ADSR (Voice Envelope)",
                                    &params.veg_attack,
                                    &params.veg_decay_adj,
                                    &params.veg_sustain,
                                    &params.veg_release,
                                    setter,
                                    true,
                                );
                            });
                        });
                    });

                    // Right-to-left layout: this lands immediately left of the
                    // module, matching the divider the flow layout used to draw.
                    ui.add_space(16.0);
                    ui.separator();
                });
            },
        );

        // Pin the child content rect to the frame bottom edge. egui's
        // `allocate_new_ui` sets the parent cursor to the child's min_rect
        // bottom (it does not take a max), and that bottom is wherever the
        // ENVELOPES module's allocation happens to end — so the PANEL_GAP
        // gutter below merged into the frame (round-19 review, issue 6
        // follow-up). Filling to the frame edge lands the cursor exactly on
        // it, and the uniform 4-logical gutter reads like every other panel
        // pair.
        let fill = (rect.bottom() - ui.cursor().top()).max(0.0);
        ui.allocate_space(vec2(1.0, fill));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Full-window case: 1346 logical row width minus the panel's 6-px inner
    /// padding on each side.
    const ROW_W: f32 = 1334.0;

    #[test]
    fn env_group_is_flush_right_at_full_width() {
        let left = env_group_left(ROW_W, LEFT_MIN_W, ENV_GROUP_W);
        assert_eq!(left, ROW_W - ENV_GROUP_W);
        // The group ends exactly on the row's right edge: no dead run beside it.
        assert_eq!(left + ENV_GROUP_W, ROW_W);
    }

    #[test]
    fn knob_strip_keeps_a_workable_column_width() {
        let left = env_group_left(ROW_W, LEFT_MIN_W, ENV_GROUP_W);
        // WAVE switch (52) + space/divider/space (13) precede the strip.
        let strip_w = left - 65.0;
        let col_w = strip_w / 7.0;
        assert!(
            (56.0..=160.0).contains(&col_w),
            "column must fit a radius-20 knob without going airy, got {col_w}"
        );
    }

    #[test]
    fn env_group_clamps_before_overlapping_the_knob_strip() {
        // Narrow row (75 % zoom): the group would start at 460 - 290 without
        // the clamp and slide over the knobs.
        let left = env_group_left(600.0, LEFT_MIN_W, ENV_GROUP_W);
        assert_eq!(left, LEFT_MIN_W);
    }
}
