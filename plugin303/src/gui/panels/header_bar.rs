use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{FontFamily, FontId, Ui};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::Plugin303Params;

pub fn render_header(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    ui.horizontal(|ui| {
        ui.set_height(40.0);

        // Logo / Title
        ui.add_space(8.0);
        ui.label(
            nih_plug_egui::egui::RichText::new("VIBE 303")
                .font(FontId::new(16.0, FontFamily::Proportional))
                .strong()
                .color(theme::TEXT_PRIMARY),
        );
        ui.label(
            nih_plug_egui::egui::RichText::new("ANALOG CIRCUIT EMULATION")
                .font(FontId::new(9.5, FontFamily::Proportional))
                .color(theme::TEXT_MUTED),
        );

        ui.with_layout(
            nih_plug_egui::egui::Layout::right_to_left(nih_plug_egui::egui::Align::Center),
            |ui| {
                ui.add_space(12.0);
                // Master Volume
                ui.add(Knob303::new(&params.volume, setter, "MASTER", 12.0));

                ui.add_space(16.0);
                // Topology switch
                ui.add(SlideSwitch::new(
                    &params.topology,
                    setter,
                    "TOPOLOGY",
                    &["ACCURATE", "FUN"],
                ));
            },
        );
    });
}
