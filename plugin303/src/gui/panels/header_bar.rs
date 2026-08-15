use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{pos2, vec2, Color32, FontFamily, FontId, Rect, Stroke, Ui};

use crate::gui::theme;
use crate::gui::widgets::SlideSwitch;
use crate::params::Plugin303Params;

pub fn render_header(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    nih_plug_egui::egui::Frame::new()
        .fill(Color32::from_rgb(0x0c, 0x12, 0x20))
        .stroke(theme::highlight_stroke())
        .corner_radius(nih_plug_egui::egui::CornerRadius::same(4))
        .inner_margin(nih_plug_egui::egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_height(28.0);

                // Logo / Title: TECHNE in primary text + O in glowing orange
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    ui.label(
                        nih_plug_egui::egui::RichText::new("TECHNE")
                            .font(FontId::new(16.0, FontFamily::Proportional))
                            .strong()
                            .color(theme::TEXT_PRIMARY),
                    );
                    ui.label(
                        nih_plug_egui::egui::RichText::new("O")
                            .font(FontId::new(16.0, FontFamily::Proportional))
                            .strong()
                            .color(theme::SOUPED_ORANGE),
                    );
                });
                ui.add_space(4.0);
                ui.label(
                    nih_plug_egui::egui::RichText::new("• VIBE 303")
                        .font(FontId::new(10.0, FontFamily::Proportional))
                        .strong()
                        .color(theme::TEXT_SECONDARY),
                );

                ui.add_space(280.0);

                // Procedural Dual 7-Slot Chassis Ventilation Grills (Centered)
                let (grill_rect, _) = ui.allocate_exact_size(vec2(140.0, 24.0), nih_plug_egui::egui::Sense::hover());
                if ui.is_rect_visible(grill_rect) {
                    let painter = ui.painter();
                    let slot_width = 3.0_f32;
                    let group_width = 56.0_f32;
                    let group_gap = grill_rect.width() - 2.0 * group_width;

                    // Render Left 7-slot group and Right 7-slot group
                    for group in 0..2 {
                        let group_start_x = grill_rect.left() + group as f32 * (group_width + group_gap);
                        let slot_gap = (group_width - 7.0 * slot_width) / 6.0;

                        for i in 0..7 {
                            let slot_x = group_start_x + i as f32 * (slot_width + slot_gap);
                            let slot = Rect::from_min_max(
                                pos2(slot_x, grill_rect.top() + 2.0),
                                pos2(slot_x + slot_width, grill_rect.bottom() - 2.0),
                            );
                            // Deep black vent recess
                            painter.rect_filled(slot, 1.0, Color32::from_rgb(0x04, 0x06, 0x0a));
                            // Bright metallic highlight stroke
                            painter.rect_stroke(
                                slot,
                                1.0,
                                Stroke::new(1.2_f32, Color32::from_rgb(0x47, 0x55, 0x69)),
                                nih_plug_egui::egui::StrokeKind::Inside,
                            );
                        }
                    }
                }

                ui.with_layout(
                    nih_plug_egui::egui::Layout::right_to_left(nih_plug_egui::egui::Align::Center),
                    |ui| {
                        ui.add_space(8.0);
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
        });
}
