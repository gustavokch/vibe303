use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{pos2, vec2, Color32, FontFamily, FontId, Rect, Stroke, Ui};

use crate::gui::theme;
use crate::gui::widgets::SlideSwitch;
use crate::params::Plugin303Params;

pub fn render_header(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    ui.horizontal(|ui| {
        ui.set_height(40.0);

        // Logo / Title
        ui.add_space(8.0);
        ui.label(
            nih_plug_egui::egui::RichText::new("TECHNEO")
                .font(FontId::new(16.0, FontFamily::Proportional))
                .strong()
                .color(theme::TEXT_PRIMARY),
        );
        ui.label(
            nih_plug_egui::egui::RichText::new("• VIBE 303")
                .font(FontId::new(10.0, FontFamily::Proportional))
                .strong()
                .color(theme::TEXT_SECONDARY),
        );

        ui.add_space(16.0);

        // Procedural Dual 7-Slot Chassis Ventilation Grills
        let (grill_rect, _) = ui.allocate_exact_size(vec2(140.0, 24.0), nih_plug_egui::egui::Sense::hover());
        if ui.is_rect_visible(grill_rect) {
            let painter = ui.painter();
            let slot_width = 2.0_f32;
            let group_width = 56.0_f32;
            let group_gap = grill_rect.width() - 2.0 * group_width;

            // Render Left 7-slot group and Right 7-slot group
            for group in 0..2 {
                let group_start_x = grill_rect.left() + group as f32 * (group_width + group_gap);
                let slot_gap = (group_width - 7.0 * slot_width) / 6.0;

                for i in 0..7 {
                    let slot_x = group_start_x + i as f32 * (slot_width + slot_gap);
                    let slot = Rect::from_min_max(
                        pos2(slot_x, grill_rect.top() + 4.0),
                        pos2(slot_x + slot_width, grill_rect.bottom() - 4.0),
                    );
                    painter.rect_filled(slot, 1.0, Color32::from_rgb(0x0e, 0x10, 0x14));
                    painter.rect_stroke(
                        slot,
                        1.0,
                        Stroke::new(0.8_f32, Color32::from_rgb(0x28, 0x2e, 0x38)),
                        nih_plug_egui::egui::StrokeKind::Inside,
                    );
                }
            }
        }

        ui.with_layout(
            nih_plug_egui::egui::Layout::right_to_left(nih_plug_egui::egui::Align::Center),
            |ui| {
                ui.add_space(16.0);
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

                ui.add_space(12.0);
                ui.label(
                    nih_plug_egui::egui::RichText::new("TB-303 PLUGIN")
                        .font(FontId::new(12.0, FontFamily::Proportional))
                        .strong()
                        .color(theme::SOUPED_ORANGE),
                );
            },
        );
    });
}
