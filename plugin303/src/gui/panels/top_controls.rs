use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{vec2, CornerRadius, Frame, Margin, Ui};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::Plugin303Params;

pub fn render_top_controls(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                // ROW 1: Classic 303 Parameter Gauges
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(14.0, 0.0);
                    ui.add(Knob303::new(&params.tuning, setter, "TUNING", 22.0));
                    ui.add(Knob303::new(&params.cutoff, setter, "CUTOFF", 22.0));
                    ui.add(Knob303::new(&params.resonance, setter, "RESONANCE", 22.0));
                    ui.add(Knob303::new(&params.env_mod, setter, "ENV MOD", 22.0));
                    ui.add(Knob303::new(&params.decay, setter, "DECAY", 22.0));
                    ui.add(Knob303::new(&params.accent, setter, "ACCENT", 22.0));
                    ui.add(Knob303::new(&params.volume, setter, "VOLUME", 22.0));
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);

                // ROW 2: Waveform Segmented Toggle
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(10.0, 0.0);

                    ui.add(SlideSwitch::new(
                        &params.waveform,
                        setter,
                        "WAVEFORM",
                        &["SAW", "SQUARE", "SWEEP"],
                    ));
                });
            });
        });
}
