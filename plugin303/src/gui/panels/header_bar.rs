use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{vec2, FontFamily, FontId, Ui};

use crate::gui::theme;
use crate::gui::widgets::SlideSwitch;
use crate::params::Plugin303Params;

pub fn render_header(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    ui.horizontal(|ui| {
        ui.set_height(22.0);

        // Logo / Title: TECHNE in primary text + O in glowing orange
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);

            let (icon_rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), nih_plug_egui::egui::Sense::hover());
            ui.painter().rect_filled(icon_rect, 2.0, theme::SOUPED_ORANGE);
            ui.add_space(4.0);

            ui.label(
                nih_plug_egui::egui::RichText::new("TECHNE")
                    .font(FontId::new(14.0, FontFamily::Proportional))
                    .strong()
                    .color(theme::TEXT_PRIMARY),
            );
            ui.label(
                nih_plug_egui::egui::RichText::new("O")
                    .font(FontId::new(14.0, FontFamily::Proportional))
                    .strong()
                    .color(theme::SOUPED_ORANGE),
            );
        });

        ui.with_layout(
            nih_plug_egui::egui::Layout::right_to_left(nih_plug_egui::egui::Align::Center),
            |ui| {
                ui.add_space(4.0);
                let prev_topology = params.topology.value();
                let switch_resp = ui.add(SlideSwitch::new(
                    &params.topology,
                    setter,
                    "CIRCUIT TOPOLOGY",
                    &["ACCURATE", "FUN"],
                ));
                if switch_resp.changed() {
                    let new_topology = params.topology.value();
                    if new_topology != prev_topology {
                        let target_comp = match new_topology {
                            crate::params::CircuitTopology::Accurate => 0.0,
                            crate::params::CircuitTopology::Fun => 0.5,
                        };
                        setter.begin_set_parameter(&params.reso_comp);
                        setter.set_parameter_normalized(&params.reso_comp, target_comp);
                        setter.end_set_parameter(&params.reso_comp);
                    }
                }
            },
        );
    });
}
