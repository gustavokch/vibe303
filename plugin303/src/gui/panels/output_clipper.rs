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
        });
}
