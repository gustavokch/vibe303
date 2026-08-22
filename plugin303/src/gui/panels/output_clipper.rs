use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{vec2, CornerRadius, FontId, Frame, Margin, Ui};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::Plugin303Params;

/// The clipper is deliberately outside the topology-gated Souped-up drawer:
/// it colours the final voice output for both Accurate and Fun engines.
pub fn render_output_clipper(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::same(6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 0.0);

                // Section title & bypass toggle
                ui.vertical(|ui| {
                    ui.label(
                        nih_plug_egui::egui::RichText::new("OUTPUT CLIPPER")
                            .font(FontId::proportional(10.0))
                            .strong()
                            .color(theme::ACCENT_RUBY),
                    );
                    ui.add_space(2.0);
                    let mut bypass = params.clipper_bypass.value();
                    let is_bypassed = bypass;
                    if ui
                        .checkbox(
                            &mut bypass,
                            nih_plug_egui::egui::RichText::new("BYPASS")
                                .font(FontId::proportional(8.5))
                                .color(if is_bypassed { theme::ACCENT_RUBY } else { theme::TEXT_SECONDARY }),
                        )
                        .changed()
                    {
                        setter.begin_set_parameter(&params.clipper_bypass);
                        setter.set_parameter(&params.clipper_bypass, bypass);
                        setter.end_set_parameter(&params.clipper_bypass);
                    }
                });

                ui.add_space(2.0);

                let bypassed = params.clipper_bypass.value();
                ui.add_enabled_ui(!bypassed, |ui| {
                    ui.add(SlideSwitch::new(&params.clipper_os, setter, "OS", &["2X", "4X"]));

                    ui.add(Knob303::new(&params.clipper_drive, setter, "DRIVE", 11.5).with_accent(theme::ACCENT_RUBY));
                    ui.add(Knob303::new(
                        &params.clipper_threshold,
                        setter,
                        "THRESH",
                        11.5,
                    ).with_accent(theme::ACCENT_RUBY));
                    ui.add(Knob303::new(&params.clipper_bass, setter, "BASS", 11.5).with_accent(theme::ACCENT_RUBY));
                    ui.add(Knob303::new(&params.clipper_tone, setter, "TONE", 11.5).with_accent(theme::ACCENT_RUBY));

                    let in_resp = ui.add(Knob303::new(&params.clipper_in_gain, setter, "IN", 11.5).with_accent(theme::ACCENT_RUBY));
                    let out_resp =
                        ui.add(Knob303::new(&params.clipper_out_gain, setter, "OUT", 11.5).with_accent(theme::ACCENT_RUBY));
                    ui.add(Knob303::new(&params.clipper_tilt, setter, "TILT", 11.5).with_accent(theme::ACCENT_RUBY));
                    ui.add(Knob303::new(&params.clipper_dynamics, setter, "DYN", 11.5).with_accent(theme::ACCENT_RUBY));
                    ui.add(Knob303::new(&params.clipper_mix, setter, "MIX", 11.5).with_accent(theme::ACCENT_RUBY));

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

                    ui.vertical(|ui| {
                        ui.add_space(4.0);
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

                        ui.add_space(4.0);

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
                                        .color(if is_selected { theme::ACCENT_RUBY } else { theme::TEXT_MUTED }),
                                )
                                .fill(if is_selected { theme::SURFACE_ELEVATED } else { theme::WELL_BG })
                                .stroke(nih_plug_egui::egui::Stroke::new(1.0_f32, if is_selected { theme::ACCENT_RUBY } else { theme::BORDER_SUBTLE }))
                                .min_size(vec2(18.0, 14.0))
                                .corner_radius(CornerRadius::same(2));

                                if ui.add(btn).on_hover_text(format!("Diode Model: {}", if is_ge { "Germanium (Asymmetric)" } else { "Silicon (Symmetric)" })).clicked() && asym != is_ge {
                                    setter.begin_set_parameter(&params.clipper_asym);
                                    setter.set_parameter(&params.clipper_asym, is_ge);
                                    setter.end_set_parameter(&params.clipper_asym);
                                }
                            }
                        });
                    });
                });
            });
        });
}
