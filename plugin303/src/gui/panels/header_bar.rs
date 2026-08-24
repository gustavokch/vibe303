use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    pos2, vec2, Align2, ComboBox, CornerRadius, FontFamily, FontId, Frame, Margin, Rect, Stroke, StrokeKind, Ui,
};

use crate::gui::theme;
use crate::gui::widgets::SlideSwitch;
use crate::params::Plugin303Params;

pub fn render_header(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let header_frame = Frame::new()
        .fill(theme::PANEL_BG)
        .corner_radius(CornerRadius::same(3))
        .stroke(theme::border_stroke())
        .inner_margin(Margin::symmetric(10, 4));

    header_frame.show(ui, |ui| {
        let bar_rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 34.0));

        // 1. Left side: Brand Logotype and Badges in dedicated container
        let left_rect = Rect::from_min_size(bar_rect.min, vec2(280.0, 34.0));
        ui.allocate_new_ui(
            nih_plug_egui::egui::UiBuilder::new().max_rect(left_rect),
            |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(
                        nih_plug_egui::egui::RichText::new("TECHNEO 303")
                            .font(FontId::new(13.0, FontFamily::Proportional))
                            .color(theme::TEXT_PRIMARY),
                    );
                    ui.add_space(8.0);

                    // BASS SYNTH badge
                    let (badge_rect, _) = ui.allocate_exact_size(vec2(72.0, 16.0), nih_plug_egui::egui::Sense::hover());
                    let painter = ui.painter();
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
                        theme::TEXT_SECONDARY,
                    );

                    ui.add_space(4.0);

                    // MONO voice mode badge
                    let (mono_rect, _) = ui.allocate_exact_size(vec2(42.0, 16.0), nih_plug_egui::egui::Sense::hover());
                    let painter = ui.painter();
                    painter.rect_filled(mono_rect, CornerRadius::same(2), theme::WELL_BG);
                    painter.rect_stroke(
                        mono_rect,
                        CornerRadius::same(2),
                        Stroke::new(1.0_f32, theme::BORDER_SUBTLE),
                        StrokeKind::Inside,
                    );
                    painter.text(
                        mono_rect.center(),
                        Align2::CENTER_CENTER,
                        "MONO",
                        FontId::new(8.0, FontFamily::Monospace),
                        theme::TEXT_SECONDARY,
                    );
                });
            },
        );

        // 2. Right control group in dedicated right-aligned container
        let right_rect = Rect::from_min_max(
            pos2(bar_rect.right() - 360.0, bar_rect.top()),
            bar_rect.right_bottom(),
        );
        ui.allocate_new_ui(
            nih_plug_egui::egui::UiBuilder::new().max_rect(right_rect),
            |ui| {
                ui.with_layout(
                    nih_plug_egui::egui::Layout::right_to_left(nih_plug_egui::egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing = vec2(12.0, 0.0);

                        // 1. Zoom factor dropdown
                        let current_zoom = ui.ctx().zoom_factor();
                        const ZOOM_PRESETS: [(f32, &str); 5] = [
                            (0.75, "75%"),
                            (1.0, "100%"),
                            (1.25, "125%"),
                            (1.5, "150%"),
                            (2.0, "200%"),
                        ];
                        let zoom_label = ZOOM_PRESETS
                            .iter()
                            .find(|(z, _)| (current_zoom - z).abs() < 0.01)
                            .map(|(_, l)| *l)
                            .unwrap_or("100%");

                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing = vec2(0.0, 1.0);
                            ui.label(
                                nih_plug_egui::egui::RichText::new("ZOOM")
                                    .font(FontId::new(8.5, FontFamily::Proportional))
                                    .color(theme::TEXT_SECONDARY),
                            );
                            ComboBox::from_id_salt("header_zoom_selector")
                                .selected_text(zoom_label)
                                .width(54.0)
                                .show_ui(ui, |ui| {
                                    for &(preset, label) in &ZOOM_PRESETS {
                                        let is_selected = (current_zoom - preset).abs() < 0.01;
                                        if ui.selectable_label(is_selected, label).clicked() {
                                            ui.ctx().set_zoom_factor(preset);
                                        }
                                    }
                                });
                        });

                        // 2. Quality selector (disabled in Accurate mode)
                        let is_accurate = params.topology.value() == crate::params::CircuitTopology::Accurate;
                        let quality_resp = ui.add_enabled_ui(!is_accurate, |ui| {
                            ui.add(
                                SlideSwitch::new(
                                    &params.tier,
                                    setter,
                                    "QUALITY",
                                    &["LOW", "HIGH", "ULTRA"],
                                )
                                .with_active_text_color(theme::ACCENT_ORANGE)
                                .with_thumb_color(theme::ACCENT_ORANGE),
                            )
                        });
                        if is_accurate {
                            quality_resp.response.on_hover_text("Quality tier applies to FUN mode (ACCURATE uses fixed physical modeling)");
                        }

                        // 3. Circuit Topology switch
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
            },
        );

        ui.advance_cursor_after_rect(bar_rect);
    });
}
