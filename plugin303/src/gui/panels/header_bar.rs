use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    pos2, vec2, Align2, ComboBox, CornerRadius, FontFamily, FontId, Frame, Margin, Rect, Stroke,
    StrokeKind, Ui,
};

use crate::gui::theme;
use crate::gui::widgets::SlideSwitch;
use crate::params::Plugin303Params;

pub(crate) const LEFT_GROUP_W: f32 = 280.0;
pub(crate) const RIGHT_GROUP_W: f32 = 480.0;

/// Compute the left offset for the right control group relative to the header bar origin.
/// Clamps to `left_width` so that at narrow widths (e.g. at 200% zoom) the right group
/// never overlaps the left brand block.
pub(crate) fn right_group_left(bar_width: f32, left_width: f32, desired: f32) -> f32 {
    (bar_width - desired).max(left_width)
}

pub fn render_header(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let header_frame = Frame::new()
        .fill(theme::PANEL_BG)
        .corner_radius(CornerRadius::same(3))
        .stroke(theme::border_stroke())
        .inner_margin(Margin::symmetric(10, 4));

    header_frame.show(ui, |ui| {
        let bar_rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 34.0));

        // 1. Left side: Brand Logotype and Badges in dedicated container
        let left_rect = Rect::from_min_size(bar_rect.min, vec2(LEFT_GROUP_W, 34.0));
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
        let right_left = bar_rect.left()
            + right_group_left(bar_rect.width(), LEFT_GROUP_W, RIGHT_GROUP_W);
        let right_rect = Rect::from_min_max(
            pos2(right_left, bar_rect.top()),
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
                        const ZOOM_PRESETS: [(f32, &str); 2] = [
                            (0.75, "75%"),
                            (1.0, "100%"),
                        ];
                        let zoom_label = ZOOM_PRESETS
                            .iter()
                            .find(|(z, _)| (current_zoom - z).abs() < 0.01)
                            .map(|(_, l)| *l)
                            .unwrap_or("100%");

                        ui.allocate_ui_with_layout(
                            // 68 = the combo's real width (text + arrow +
                            // button padding). The allocation must hug the
                            // control: a smaller box centring-overflowed past
                            // the bar's right content edge, a larger one left
                            // a dead gap (round-11 item 9).
                            vec2(68.0, 34.0),
                            // Centred like TOPOLOGY/QUALITY (round-11 item 17).
                            nih_plug_egui::egui::Layout::top_down(nih_plug_egui::egui::Align::Center),
                            |ui| {
                                ui.spacing_mut().item_spacing = vec2(0.0, 1.0);
                                ui.label(
                                    nih_plug_egui::egui::RichText::new("ZOOM")
                                        .font(FontId::new(8.5, FontFamily::Proportional))
                                        .color(theme::TEXT_SECONDARY),
                                );
                                ComboBox::from_id_salt("header_zoom_selector")
                                    // Header caption class (8.5 pt) for the
                                    // button text and popup items (round-13
                                    // issue 8).
                                    .selected_text(
                                        nih_plug_egui::egui::RichText::new(zoom_label)
                                            .font(FontId::new(8.5, FontFamily::Proportional)),
                                    )
                                    .width(54.0)
                                    .show_ui(ui, |ui| {
                                        for &(preset, label) in &ZOOM_PRESETS {
                                            let is_selected = (current_zoom - preset).abs() < 0.01;
                                            if ui
                                                .selectable_label(
                                                    is_selected,
                                                    nih_plug_egui::egui::RichText::new(label)
                                                        .font(FontId::new(
                                                            8.5,
                                                            FontFamily::Proportional,
                                                        )),
                                                )
                                                .clicked()
                                            {
                                                ui.ctx().set_zoom_factor(preset);
                                            }
                                        }
                                    });
                            },
                        );

                        // 2. Quality selector (disabled in Accurate mode)
                        let is_accurate = params.topology.value() == crate::params::CircuitTopology::Accurate;
                        let quality_resp = ui.add_enabled_ui(!is_accurate, |ui| {
                            ui.add(
                                SlideSwitch::new(
                                    &params.tier,
                                    setter,
                                    "QUALITY",
                                    &["LOW", "HIGH", "ULTRA", "ULTRA 2X"],
                                ),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_group_left_wide_case_no_clamp() {
        // Wide window: 1346 px available width, left=280, desired=480
        // Expected left offset = 1346 - 480 = 866 >= 280
        let left = right_group_left(1346.0, LEFT_GROUP_W, RIGHT_GROUP_W);
        assert_eq!(left, 1346.0 - RIGHT_GROUP_W);
        assert!(left > LEFT_GROUP_W);
    }

    #[test]
    fn right_group_left_exact_boundary() {
        // Exact boundary: bar_width = left_width + desired = 760.0
        let left = right_group_left(LEFT_GROUP_W + RIGHT_GROUP_W, LEFT_GROUP_W, RIGHT_GROUP_W);
        assert_eq!(left, LEFT_GROUP_W);
    }

    #[test]
    fn right_group_left_narrow_zoomed_clamped_no_overlap() {
        // Constrained logical width ~640 pt, desired 480 pt -> would be 160 without clamp.
        // Clamps to left_width (280.0) so it never overlaps left brand block.
        let left = right_group_left(640.0, LEFT_GROUP_W, RIGHT_GROUP_W);
        assert_eq!(left, LEFT_GROUP_W);
    }
}
