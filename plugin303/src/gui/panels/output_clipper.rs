use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{Color32, Frame, Margin, Ui};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::Plugin303Params;

/// The clipper is deliberately outside the topology-gated Souped-up drawer:
/// it colours the final voice output for both Accurate and Fun engines.
pub fn render_output_clipper(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .inner_margin(Margin::same(7))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(nih_plug_egui::egui::RichText::new("OUTPUT CLIPPER").strong().color(theme::SOUPED_ORANGE));
                let mut bypass = params.clipper_bypass.value();
                if ui.checkbox(&mut bypass, "BYPASS").changed() {
                    setter.begin_set_parameter(&params.clipper_bypass);
                    setter.set_parameter(&params.clipper_bypass, bypass);
                    setter.end_set_parameter(&params.clipper_bypass);
                }
                ui.add(SlideSwitch::new(&params.clipper_os, setter, "OS", &["2X", "4X"]));
                ui.add(Knob303::new(&params.clipper_drive, setter, "DRIVE", 13.0));
                ui.add(Knob303::new(&params.clipper_threshold, setter, "THRESH", 13.0));
                ui.add(Knob303::new(&params.clipper_bass, setter, "BASS", 13.0));
                ui.add(Knob303::new(&params.clipper_tone, setter, "TONE", 13.0));
                let mut asym = params.clipper_asym.value();
                if ui.checkbox(&mut asym, "ASYM").changed() {
                    setter.begin_set_parameter(&params.clipper_asym);
                    setter.set_parameter(&params.clipper_asym, asym);
                    setter.end_set_parameter(&params.clipper_asym);
                }
                ui.colored_label(Color32::from_gray(120), "GE / JFET");
            });

            let bypassed = params.clipper_bypass.value();
            ui.add_enabled_ui(!bypassed, |ui| {
                ui.horizontal(|ui| {
                    ui.add(Knob303::new(&params.clipper_in_gain, setter, "IN", 13.0));
                    let mut link = params.clipper_gain_link.value();
                    if ui.checkbox(&mut link, "LINK").changed() {
                        setter.begin_set_parameter(&params.clipper_gain_link);
                        setter.set_parameter(&params.clipper_gain_link, link);
                        setter.end_set_parameter(&params.clipper_gain_link);
                        if link {
                            // Snap OUT to the inverse of IN when link engages.
                            setter.begin_set_parameter(&params.clipper_out_gain);
                            setter.set_parameter(&params.clipper_out_gain, -params.clipper_in_gain.value());
                            setter.end_set_parameter(&params.clipper_out_gain);
                        }
                    }
                    ui.add(Knob303::new(&params.clipper_out_gain, setter, "OUT", 13.0));
                    ui.add(Knob303::new(&params.clipper_tilt, setter, "TILT", 13.0));
                    ui.add(Knob303::new(&params.clipper_dynamics, setter, "DYN", 13.0));
                    ui.add(Knob303::new(&params.clipper_mix, setter, "MIX", 13.0));
                });
            });

            // Gain link enforcement: edit-time only. Compare against the values
            // seen last frame; whichever knob moved drives its counterpart.
            if params.clipper_gain_link.value() {
                let id = ui.id().with("clipper_gain_link_last");
                let last: Option<(f32, f32)> = ui.ctx().data_mut(|d| d.get_temp(id));
                let cur_in = params.clipper_in_gain.value();
                let cur_out = params.clipper_out_gain.value();
                if let Some((li, lo)) = last {
                    if cur_in != li {
                        setter.begin_set_parameter(&params.clipper_out_gain);
                        setter.set_parameter(&params.clipper_out_gain, -cur_in);
                        setter.end_set_parameter(&params.clipper_out_gain);
                    } else if cur_out != lo {
                        setter.begin_set_parameter(&params.clipper_in_gain);
                        setter.set_parameter(&params.clipper_in_gain, -cur_out);
                        setter.end_set_parameter(&params.clipper_in_gain);
                    }
                }
                ui.ctx().data_mut(|d| {
                    d.insert_temp(id, (params.clipper_in_gain.value(), params.clipper_out_gain.value()));
                });
            }
        });
}
