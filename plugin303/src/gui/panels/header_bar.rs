use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{pos2, vec2, Align2, ComboBox, CornerRadius, FontFamily, FontId, Rect, Stroke, StrokeKind, Ui};

use crate::gui::theme;
use crate::gui::widgets::SlideSwitch;
use crate::params::Plugin303Params;

pub fn render_header(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let content_rect = ui.max_rect();
    let strip_top = ui.cursor().min.y;

    // Flat header container
    let header_rect = Rect::from_min_max(
        pos2(content_rect.left(), strip_top),
        pos2(content_rect.right(), strip_top + 28.0),
    );

    {
        let painter = ui.painter();
        painter.rect_filled(header_rect, CornerRadius::same(3), theme::PANEL_BG);
        painter.rect_stroke(
            header_rect,
            CornerRadius::same(3),
            Stroke::new(1.0_f32, theme::BORDER_DEFAULT),
            StrokeKind::Inside,
        );

        // Left: Clean Flat Brand Logotype
        painter.text(
            pos2(header_rect.left() + 10.0, header_rect.center().y),
            Align2::LEFT_CENTER,
            "TECHNEO 303",
            FontId::new(14.0, FontFamily::Proportional),
            theme::TEXT_PRIMARY,
        );

        let badge_rect = Rect::from_min_size(
            pos2(header_rect.left() + 115.0, header_rect.center().y - 7.0),
            vec2(76.0, 14.0),
        );
        painter.rect_filled(badge_rect, CornerRadius::same(2), theme::WELL_BG);
        painter.rect_stroke(
            badge_rect,
            CornerRadius::same(2),
            Stroke::new(1.0_f32, theme::BORDER_SUBTLE),
            StrokeKind::Inside,
        );
        painter.text(
            badge_rect.center(),
            Align2::CENTER_CENTER,
            "BASS SYNTH",
            FontId::new(8.0, FontFamily::Monospace),
            theme::ACCENT_ORANGE,
        );
    }

    ui.horizontal(|ui| {
        ui.set_height(28.0);
        ui.add_space(200.0);

        // Right-aligned controls
        ui.with_layout(
            nih_plug_egui::egui::Layout::right_to_left(nih_plug_egui::egui::Align::Center),
            |ui| {
                ui.add_space(8.0);

                // Zoom factor dropdown
                let current_zoom = ui.ctx().zoom_factor();
                let current_pct = (current_zoom * 100.0).round() as i32;
                let zoom_label = format!("{}%", current_pct);

                ComboBox::from_id_salt("header_zoom_selector")
                    .selected_text(zoom_label)
                    .width(64.0)
                    .show_ui(ui, |ui| {
                        let zoom_presets = [0.75_f32, 1.0, 1.25, 1.5, 2.0];
                        for preset in zoom_presets {
                            let pct = (preset * 100.0).round() as i32;
                            let is_selected = (current_zoom - preset).abs() < 0.01;
                            if ui.selectable_label(is_selected, format!("{}%", pct)).clicked() {
                                ui.ctx().set_zoom_factor(preset);
                            }
                        }
                    });

                ui.label(
                    nih_plug_egui::egui::RichText::new("ZOOM")
                        .font(FontId::new(9.0, FontFamily::Proportional))
                        .color(theme::TEXT_SECONDARY),
                );

                ui.add_space(12.0);

                // Quality selector (disabled in Accurate mode)
                let is_accurate = params.topology.value() == crate::params::CircuitTopology::Accurate;
                ui.add_enabled_ui(!is_accurate, |ui| {
                    ui.add(SlideSwitch::new(
                        &params.tier,
                        setter,
                        "QUALITY",
                        &["LOW", "HIGH", "ULTRA"],
                    ));
                });

                ui.add_space(8.0);

                // Circuit Topology switch
                let prev_topology = params.topology.value();
                let switch_resp = ui.add(SlideSwitch::new(
                    &params.topology,
                    setter,
                    "TOPOLOGY",
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
