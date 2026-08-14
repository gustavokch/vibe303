use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{CornerRadius, Frame, Margin, Ui};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::Plugin303Params;

pub fn render_classic(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.set_height(220.0);
            ui.set_width(510.0);

            ui.label(
                nih_plug_egui::egui::RichText::new("CLASSIC 303 FRONT PANEL")
                    .font(nih_plug_egui::egui::FontId::new(11.0, nih_plug_egui::egui::FontFamily::Proportional))
                    .strong()
                    .color(theme::TEXT_SECONDARY),
            );

            ui.add_space(6.0);

            // Row of main dials
            ui.horizontal(|ui| {
                ui.add(Knob303::new(&params.tuning, setter, "TUNING", 16.0));
                ui.add(Knob303::new(&params.cutoff, setter, "CUTOFF", 20.0));
                ui.add(Knob303::new(&params.resonance, setter, "RESO", 20.0));
                ui.add(Knob303::new(&params.env_mod, setter, "ENV MOD", 18.0));
                ui.add(Knob303::new(&params.decay, setter, "DECAY", 18.0));
                ui.add(Knob303::new(&params.accent, setter, "ACCENT", 18.0));
            });

            ui.add_space(10.0);

            // Switches: Waveform & Tier
            ui.horizontal(|ui| {
                ui.add(SlideSwitch::new(
                    &params.waveform,
                    setter,
                    "WAVEFORM",
                    &["SAW", "SQR", "SWEEP"],
                ));
                ui.add_space(20.0);
                ui.add(SlideSwitch::new(
                    &params.tier,
                    setter,
                    "FIDELITY TIER",
                    &["LOW", "HIGH", "ULTRA"],
                ));
            });
        });
}
