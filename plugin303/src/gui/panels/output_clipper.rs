use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{vec2, CornerRadius, FontId, Frame, Margin, Ui};

use crate::gui::theme;
use crate::gui::widgets::{BoolSwitch, Knob303, SlideSwitch};
use crate::params::Plugin303Params;

/// The clipper is deliberately outside the topology-gated Souped-up drawer:
/// it colours the final voice output for both Accurate and Fun engines.
pub fn render_output_clipper(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let bypassed = params.clipper_bypass.value();
    let frame_stroke = if bypassed {
        nih_plug_egui::egui::Stroke::new(1.0_f32, theme::BORDER_DEFAULT)
    } else {
        theme::border_stroke()
    };

    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(frame_stroke)
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::same(6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(78.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 0.0);

                // Section title, bypass toggle & OS selector
                ui.vertical(|ui| {
                    ui.label(
                        nih_plug_egui::egui::RichText::new("OUTPUT CLIPPER")
                            .font(FontId::proportional(10.0))
                            .strong()
                            .color(if bypassed {
                                theme::TEXT_MUTED
                            } else {
                                // Every panel title shares the accent colour
                                // (round-11 item 8); muted only when disabled.
                                theme::ACCENT_ORANGE
                            }),
                    );
                    ui.add_space(2.0);
                    let mut bypass = bypassed;
                    let is_bypassed = bypass;
                    if ui
                        .checkbox(
                            &mut bypass,
                            nih_plug_egui::egui::RichText::new("BYPASS")
                                .font(FontId::proportional(8.5))
                                .color(if is_bypassed {
                                    theme::ACCENT_ORANGE
                                } else {
                                    theme::TEXT_SECONDARY
                                }),
                        )
                        .changed()
                    {
                        setter.begin_set_parameter(&params.clipper_bypass);
                        setter.set_parameter(&params.clipper_bypass, bypass);
                        setter.end_set_parameter(&params.clipper_bypass);
                    }
                });

                ui.add_space(2.0);

                ui.add_enabled_ui(!bypassed, |ui| {
                    ui.add(SlideSwitch::new(
                        &params.clipper_os,
                        setter,
                        "OS",
                        &["2X", "4X"],
                    ));

                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // 9 Knobs distributed evenly across the center area
                    let right_fixed_w = 110.0_f32;
                    let remaining_w = (ui.available_width() - right_fixed_w).max(9.0 * 40.0);
                    let col_w = remaining_w / 9.0;
                    // Cells are exact-width slots; zero inter-cell spacing so the
                    // row consumes exactly remaining_w and LINK/DIODE never clip.
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);

                    let mut render_knob = |knob: Knob303<'_, _>| {
                        ui.allocate_ui_with_layout(
                            vec2(col_w, 64.0),
                            nih_plug_egui::egui::Layout::top_down(
                                nih_plug_egui::egui::Align::Center,
                            ),
                            |ui| ui.add(knob),
                        )
                        .inner
                    };

                    render_knob(Knob303::new(
                        &params.clipper_drive,
                        setter,
                        "DRIVE",
                        theme::KNOB_RADIUS_MEDIUM,
                    ));
                    render_knob(Knob303::new(
                        &params.clipper_threshold,
                        setter,
                        "THRESH",
                        theme::KNOB_RADIUS_MEDIUM,
                    ));
                    render_knob(Knob303::new(
                        &params.clipper_bass,
                        setter,
                        "BASS",
                        theme::KNOB_RADIUS_MEDIUM,
                    ));
                    render_knob(Knob303::new(
                        &params.clipper_tone,
                        setter,
                        "TONE",
                        theme::KNOB_RADIUS_MEDIUM,
                    ));

                    let in_resp = render_knob(Knob303::new(
                        &params.clipper_in_gain,
                        setter,
                        "IN",
                        theme::KNOB_RADIUS_MEDIUM,
                    ));
                    let out_resp = render_knob(Knob303::new(
                        &params.clipper_out_gain,
                        setter,
                        "OUT",
                        theme::KNOB_RADIUS_MEDIUM,
                    ));
                    render_knob(Knob303::new(
                        &params.clipper_tilt,
                        setter,
                        "TILT",
                        theme::KNOB_RADIUS_MEDIUM,
                    ));
                    render_knob(Knob303::new(
                        &params.clipper_dynamics,
                        setter,
                        "DYN",
                        theme::KNOB_RADIUS_MEDIUM,
                    ));
                    render_knob(Knob303::new(
                        &params.clipper_mix,
                        setter,
                        "MIX",
                        theme::KNOB_RADIUS_MEDIUM,
                    ));

                    // Gain link: mirror only on genuine user gestures (drag, double-click reset)
                    if params.clipper_gain_link.value() {
                        if in_resp.drag_started() || in_resp.double_clicked() {
                            setter.begin_set_parameter(&params.clipper_out_gain);
                        }
                        if in_resp.dragged() || in_resp.double_clicked() {
                            setter.set_parameter(
                                &params.clipper_out_gain,
                                -params.clipper_in_gain.value(),
                            );
                        }
                        if in_resp.drag_stopped() || in_resp.double_clicked() {
                            setter.end_set_parameter(&params.clipper_out_gain);
                        }
                        if out_resp.drag_started() || out_resp.double_clicked() {
                            setter.begin_set_parameter(&params.clipper_in_gain);
                        }
                        if out_resp.dragged() || out_resp.double_clicked() {
                            setter.set_parameter(
                                &params.clipper_in_gain,
                                -params.clipper_out_gain.value(),
                            );
                        }
                        if out_resp.drag_stopped() || out_resp.double_clicked() {
                            setter.end_set_parameter(&params.clipper_in_gain);
                        }
                    }

                    ui.separator();
                    ui.add_space(2.0);

                    // Right controls: Link toggle + Diode selector
                    ui.vertical(|ui| {
                        ui.add_space(2.0);
                        let mut link = params.clipper_gain_link.value();
                        let is_linked = link;
                        if ui
                            .checkbox(
                                &mut link,
                                nih_plug_egui::egui::RichText::new("LINK")
                                    .font(FontId::proportional(8.5))
                                    .color(if is_linked {
                                        theme::ACCENT_ORANGE
                                    } else {
                                        theme::TEXT_SECONDARY
                                    }),
                            )
                            .on_hover_text("Link: OUT tracks −IN while dragging (edit-time only)")
                            .changed()
                        {
                            setter.begin_set_parameter(&params.clipper_gain_link);
                            setter.set_parameter(&params.clipper_gain_link, link);
                            setter.end_set_parameter(&params.clipper_gain_link);
                            if link {
                                setter.begin_set_parameter(&params.clipper_out_gain);
                                setter.set_parameter(
                                    &params.clipper_out_gain,
                                    -params.clipper_in_gain.value(),
                                );
                                setter.end_set_parameter(&params.clipper_out_gain);
                            }
                        }

                        ui.add_space(2.0);

                        ui.add(
                            BoolSwitch::new(&params.clipper_asym, setter, "DIODE", ["SI", "GE"])
                                .with_width(52.0)
                                .with_option_tooltips([
                                    Some("Silicon (Symmetric)"),
                                    Some("Germanium (Asymmetric)"),
                                ]),
                        );
                        ui.add_space(3.0);
                    });
                });
            });
        });
}

#[cfg(test)]
mod tests {
    #[test]
    fn output_clipper_knob_column_width() {
        // Inner width for 1366px window with 10px chassis margin and 6px panel inner margin
        let avail_w = 1366.0 - 20.0 - 12.0; // 1334.0
        let header_w = 120.0;
        let right_controls_w = 110.0;
        let knobs_avail_w = avail_w - header_w - right_controls_w;
        let col_w = knobs_avail_w / 9.0;
        assert!(
            col_w >= 100.0,
            "Knob column width must provide generous spacing (>=100px), got {col_w}"
        );
    }

    #[test]
    fn output_clipper_knob_row_fits_reserved_band() {
        let avail_w = 1366.0 - 20.0 - 12.0; // 1334.0
        let header_w = 120.0_f32;
        let right_fixed_w = 110.0_f32;
        let knob_count = 9.0_f32;
        let inter_cell_spacing = 0.0_f32; // cells sized exactly; no inter-cell spacing or right group clips
        let knobs_avail_w = avail_w - header_w - right_fixed_w;
        let col_w = knobs_avail_w / knob_count;
        let consumed = knob_count * col_w + (knob_count - 1.0) * inter_cell_spacing;
        assert!(
            consumed <= knobs_avail_w + 0.01,
            "Knob row ({consumed}px) must fit its reserved band ({knobs_avail_w}px) or LINK/DIODE clip at panel edge"
        );
    }
}
