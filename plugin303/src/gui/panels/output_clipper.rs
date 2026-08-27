use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{vec2, CornerRadius, FontId, Frame, Margin, Ui};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::Plugin303Params;

/// The clipper is deliberately outside the topology-gated Souped-up drawer:
/// it colours the final voice output for both Accurate and Fun engines.
pub fn render_output_clipper(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let bypassed = params.clipper_bypass.value();
    let frame_stroke = if bypassed {
        nih_plug_egui::egui::Stroke::new(1.0_f32, theme::BORDER_SUBTLE)
    } else {
        theme::border_stroke()
    };

    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(frame_stroke)
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::same(6))
        .show(ui, |ui| {
            ui.set_min_height(78.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 0.0);

                // Section title & bypass toggle
                ui.vertical(|ui| {
                    ui.label(
                        nih_plug_egui::egui::RichText::new("OUTPUT CLIPPER")
                            .font(FontId::proportional(10.0))
                            .strong()
                            .color(if bypassed { theme::TEXT_MUTED } else { theme::TEXT_PRIMARY }),
                    );
                    ui.add_space(2.0);
                    let mut bypass = bypassed;
                    let is_bypassed = bypass;
                    if ui
                        .checkbox(
                            &mut bypass,
                            nih_plug_egui::egui::RichText::new("BYPASS")
                                .font(FontId::proportional(8.5))
                                .color(if is_bypassed { theme::ACCENT_ORANGE } else { theme::TEXT_SECONDARY }),
                        )
                        .changed()
                    {
                        setter.begin_set_parameter(&params.clipper_bypass);
                        setter.set_parameter(&params.clipper_bypass, bypass);
                        setter.end_set_parameter(&params.clipper_bypass);
                    }
                });

                ui.add_space(2.0);

                // Right-hand controls distributed across the full remaining
                // width in fixed columns (same convention as the souped
                // panel): OS | 9 knobs | LINK+DIODE, separated by thin
                // vertical rules. No oversized inter-group gaps (fix #6).
                ui.add_enabled_ui(!bypassed, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    const GROUP_SEP_W: f32 = 2.0;
                    const UNITS: f32 = 11.0; // 1 (OS) + 9 (knobs) + 1 (LINK/DIODE)
                    let total_w = ui.available_width();
                    let col_w = (total_w - 2.0 * GROUP_SEP_W) / UNITS;

                    // Group rules are painted 2px columns, NOT egui Separators
                    // (Separator::default() allocates ~6px and overflows the row).
                    let paint_group_rule = |ui: &mut Ui| {
                        let (rule_rect, _) = ui.allocate_exact_size(
                            vec2(GROUP_SEP_W, 62.0),
                            nih_plug_egui::egui::Sense::hover(),
                        );
                        if ui.is_rect_visible(rule_rect) {
                            ui.painter().vline(
                                rule_rect.center().x,
                                rule_rect.y_range(),
                                nih_plug_egui::egui::Stroke::new(1.0_f32, theme::BORDER_SUBTLE),
                            );
                        }
                    };

                    let render_knob = |ui: &mut Ui, knob: Knob303<'_, _>| {
                        ui.allocate_ui_with_layout(
                            vec2(col_w, 62.0),
                            nih_plug_egui::egui::Layout::top_down(nih_plug_egui::egui::Align::Center),
                            |ui| ui.add(knob),
                        )
                        .inner
                    };

                    // Group 1: OS switch
                    ui.allocate_ui_with_layout(
                        vec2(col_w, 62.0),
                        nih_plug_egui::egui::Layout::top_down(nih_plug_egui::egui::Align::Center),
                        |ui| {
                            ui.add(SlideSwitch::new(
                                &params.clipper_os,
                                setter,
                                "OS",
                                &["2X", "4X"],
                            ));
                        },
                    );

                    paint_group_rule(ui);

                    // Group 2: knob row
                    render_knob(ui, Knob303::new(&params.clipper_drive, setter, "DRIVE", 11.5));
                    render_knob(ui, Knob303::new(&params.clipper_threshold, setter, "THRESH", 11.5));
                    render_knob(ui, Knob303::new(&params.clipper_bass, setter, "BASS", 11.5));
                    render_knob(ui, Knob303::new(&params.clipper_tone, setter, "TONE", 11.5));
                    let in_resp = render_knob(ui, Knob303::new(&params.clipper_in_gain, setter, "IN", 11.5));
                    let out_resp = render_knob(ui, Knob303::new(&params.clipper_out_gain, setter, "OUT", 11.5));
                    render_knob(ui, Knob303::new(&params.clipper_tilt, setter, "TILT", 11.5));
                    render_knob(ui, Knob303::new(&params.clipper_dynamics, setter, "DYN", 11.5));
                    render_knob(ui, Knob303::new(&params.clipper_mix, setter, "MIX", 11.5));

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

                    paint_group_rule(ui);

                    // Group 3: LINK + DIODE
                    ui.allocate_ui_with_layout(
                        vec2(col_w, 62.0),
                        nih_plug_egui::egui::Layout::top_down(nih_plug_egui::egui::Align::Center),
                        |ui| {
                            ui.vertical(|ui| {
                                ui.add_space(2.0);
                                let mut link = params.clipper_gain_link.value();
                                if ui
                                    .checkbox(
                                        &mut link,
                                        nih_plug_egui::egui::RichText::new("LINK")
                                            .font(FontId::proportional(8.5))
                                            .color(theme::TEXT_SECONDARY),
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

                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                                    ui.label(
                                        nih_plug_egui::egui::RichText::new("DIODE:")
                                            .font(FontId::proportional(8.0))
                                            .color(theme::TEXT_MUTED),
                                    );
                                    let asym = params.clipper_asym.value();
                                    for (is_ge, label) in [(false, "SI"), (true, "GE")] {
                                        let is_selected = asym == is_ge;
                                        let btn = nih_plug_egui::egui::Button::new(
                                            nih_plug_egui::egui::RichText::new(label)
                                                .font(FontId::proportional(8.0))
                                                .strong()
                                                .color(if is_selected { theme::ACCENT_ORANGE } else { theme::TEXT_SECONDARY }),
                                        )
                                        .fill(if is_selected { theme::SURFACE_ELEVATED } else { theme::WELL_BG })
                                        .stroke(nih_plug_egui::egui::Stroke::new(1.0_f32, if is_selected { theme::ACCENT_ORANGE } else { theme::BORDER_DEFAULT }))
                                        .min_size(vec2(18.0, 14.0))
                                        .corner_radius(CornerRadius::same(2));

                                        if ui.add(btn).on_hover_text(format!("Diode Model: {}", if is_ge { "Germanium (Asymmetric)" } else { "Silicon (Symmetric)" })).clicked() && asym != is_ge {
                                            setter.begin_set_parameter(&params.clipper_asym);
                                            setter.set_parameter(&params.clipper_asym, is_ge);
                                            setter.end_set_parameter(&params.clipper_asym);
                                        }
                                    }
                                });
                                ui.add_space(3.0);
                            });
                        },
                    );
                });
            });
        });
}
