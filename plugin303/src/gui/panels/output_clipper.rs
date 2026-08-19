use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{vec2, Color32, Frame, Margin, Ui};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::Plugin303Params;

/// The clipper is deliberately outside the topology-gated Souped-up drawer:
/// it colours the final voice output for both Accurate and Fun engines.
pub fn render_output_clipper(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .inner_margin(Margin::same(6))
        .show(ui, |ui| {
            // Single-row layout: keeps the panel short enough for the whole
            // window to fit the fixed 1080x640 frame.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 0.0);

                ui.vertical(|ui| {
                    ui.label(
                        nih_plug_egui::egui::RichText::new("OUTPUT CLIPPER")
                            .strong()
                            .color(theme::SOUPED_ORANGE),
                    );
                    let mut bypass = params.clipper_bypass.value();
                    if ui.checkbox(&mut bypass, "BYPASS").changed() {
                        setter.begin_set_parameter(&params.clipper_bypass);
                        setter.set_parameter(&params.clipper_bypass, bypass);
                        setter.end_set_parameter(&params.clipper_bypass);
                    }
                });

                ui.add(SlideSwitch::new(&params.clipper_os, setter, "OS", &["2X", "4X"]));

                ui.add(Knob303::new(&params.clipper_drive, setter, "DRIVE", 12.0));
                ui.add(Knob303::new(
                    &params.clipper_threshold,
                    setter,
                    "THRESH",
                    12.0,
                ));
                ui.add(Knob303::new(&params.clipper_bass, setter, "BASS", 12.0));
                ui.add(Knob303::new(&params.clipper_tone, setter, "TONE", 12.0));

                let bypassed = params.clipper_bypass.value();
                ui.add_enabled_ui(!bypassed, |ui| {
                    let in_resp = ui.add(Knob303::new(&params.clipper_in_gain, setter, "IN", 12.0));
                    let out_resp =
                        ui.add(Knob303::new(&params.clipper_out_gain, setter, "OUT", 12.0));
                    ui.add(Knob303::new(&params.clipper_tilt, setter, "TILT", 12.0));
                    ui.add(Knob303::new(&params.clipper_dynamics, setter, "DYN", 12.0));
                    ui.add(Knob303::new(&params.clipper_mix, setter, "MIX", 12.0));

                    // Gain link: mirror only on genuine user gestures (drag,
                    // double-click reset). Host-driven changes (preset recall,
                    // automation) produce no widget events, so they never
                    // trigger counterpart writes.
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
                });

                ui.vertical(|ui| {
                    let mut link = params.clipper_gain_link.value();
                    if ui
                        .checkbox(&mut link, "LINK")
                        .on_hover_text("Link: OUT tracks −IN while dragging (edit-time only)")
                        .changed()
                    {
                        setter.begin_set_parameter(&params.clipper_gain_link);
                        setter.set_parameter(&params.clipper_gain_link, link);
                        setter.end_set_parameter(&params.clipper_gain_link);
                        if link {
                            // Snap OUT to the inverse of IN when link engages.
                            setter.begin_set_parameter(&params.clipper_out_gain);
                            setter.set_parameter(
                                &params.clipper_out_gain,
                                -params.clipper_in_gain.value(),
                            );
                            setter.end_set_parameter(&params.clipper_out_gain);
                        }
                    }
                    let mut asym = params.clipper_asym.value();
                    if ui.checkbox(&mut asym, "ASYM").changed() {
                        setter.begin_set_parameter(&params.clipper_asym);
                        setter.set_parameter(&params.clipper_asym, asym);
                        setter.end_set_parameter(&params.clipper_asym);
                    }
                    ui.colored_label(Color32::from_gray(120), "GE / JFET");
                });
            });
        });
}
