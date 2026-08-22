use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    pos2, vec2, Align2, Button, Color32, CornerRadius, FontId, Frame, Margin, Rect, Stroke, Ui,
};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::{CircuitTopology, Plugin303Params};

pub fn render_souped_panel(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let is_accurate = params.topology.value() == CircuitTopology::Accurate;

    let response = Frame::new()
        .fill(theme::PANEL_BG_SOUPED)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(86.0);
            ui.vertical(|ui| {
                // Header Bar
                ui.horizontal(|ui| {
                    ui.label(
                        nih_plug_egui::egui::RichText::new("SOUPED-UP ENGINE & ANALOG DRAWER")
                            .font(FontId::new(10.0, nih_plug_egui::egui::FontFamily::Proportional))
                            .strong()
                            .color(if is_accurate {
                                theme::TEXT_MUTED
                            } else {
                                theme::ACCENT_ORANGE
                            }),
                    );
                });

                ui.add_space(3.0);

                // Row 1: Selectors & Mode Toggles
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(10.0, 0.0);

                    ui.add(SlideSwitch::new(
                        &params.vcf_taps,
                        setter,
                        "VCF TAPS",
                        &["2-POLE", "3-POLE", "4-POLE"],
                    ));

                    ui.add_space(8.0);

                    ui.add(SlideSwitch::new(
                        &params.vca_env_mode,
                        setter,
                        "VCA ENV",
                        &["GATE", "ENV"],
                    ));
                });

                ui.add_space(2.0);
                ui.separator();
                ui.add_space(2.0);

                // Row 2: Engine Rotary Dials (spaced evenly across full width)
                ui.horizontal(|ui| {
                    let num_knobs = 9;
                    let avail_w = ui.available_width();
                    let spacing = ((avail_w - (num_knobs as f32 * 42.0)) / (num_knobs as f32 + 1.0)).max(12.0);
                    ui.spacing_mut().item_spacing = vec2(spacing, 0.0);
                    ui.add_space(spacing * 0.5);

                    ui.add(Knob303::new(&params.vcf_drive, setter, "DRIVE", 12.0));
                    ui.add(Knob303::new(&params.sub_vol, setter, "SUB", 12.0));
                    ui.add(Knob303::new(&params.glide, setter, "GLIDE", 12.0));
                    ui.add(Knob303::new(&params.osc1_pitch, setter, "PITCH1", 12.0));
                    ui.add(Knob303::new(&params.osc2_pitch, setter, "PITCH2", 12.0));
                    ui.add(Knob303::new(&params.osc2_vol, setter, "MIX2", 12.0));
                    ui.add(Knob303::new(&params.osc1_pw, setter, "PW", 12.0));
                    ui.add(Knob303::new(&params.sweep_pos, setter, "SWEEP", 12.0));
                    ui.add(Knob303::new(&params.reso_comp, setter, "R-COMP", 12.0));
                });
            });
        });

    // Accurate Mode Overlay Badge
    if is_accurate {
        let overlay_rect = response.response.rect;

        let painter = ui.painter();
        painter.rect_filled(
            overlay_rect,
            CornerRadius::same(3),
            Color32::from_rgba_premultiplied(0x13, 0x13, 0x15, 0xd8),
        );

        let center = overlay_rect.center();
        let badge_width = 280.0;
        let badge_height = 56.0;
        let badge_rect = Rect::from_center_size(center, vec2(badge_width, badge_height));

        painter.rect_filled(badge_rect, CornerRadius::same(3), theme::SURFACE_ELEVATED);
        painter.rect_stroke(
            badge_rect,
            CornerRadius::same(3),
            Stroke::new(1.0_f32, theme::BORDER_FOCUS),
            nih_plug_egui::egui::StrokeKind::Inside,
        );

        painter.text(
            pos2(center.x, center.y - 14.0),
            Align2::CENTER_CENTER,
            "ACCURATE MODE ACTIVE",
            FontId::new(10.5, nih_plug_egui::egui::FontFamily::Proportional),
            theme::TEXT_PRIMARY,
        );

        painter.text(
            pos2(center.x, center.y - 1.0),
            Align2::CENTER_CENTER,
            "Souped-up engine is bypassed",
            FontId::new(8.5, nih_plug_egui::egui::FontFamily::Proportional),
            theme::TEXT_MUTED,
        );

        let btn_rect = Rect::from_center_size(pos2(center.x, center.y + 15.0), vec2(120.0, 18.0));
        let btn_resp = ui.put(
            btn_rect,
            Button::new(
                nih_plug_egui::egui::RichText::new("ENABLE FUN MODE")
                    .font(FontId::new(8.5, nih_plug_egui::egui::FontFamily::Proportional))
                    .strong()
                    .color(theme::ACCENT_ORANGE),
            )
            .fill(theme::WELL_BG)
            .stroke(Stroke::new(1.0_f32, theme::BORDER_DEFAULT))
            .corner_radius(CornerRadius::same(2)),
        );

        if btn_resp.clicked() {
            setter.begin_set_parameter(&params.topology);
            setter.set_parameter(&params.topology, CircuitTopology::Fun);
            setter.end_set_parameter(&params.topology);

            setter.begin_set_parameter(&params.reso_comp);
            setter.set_parameter_normalized(&params.reso_comp, 0.5);
            setter.end_set_parameter(&params.reso_comp);
        }
    }
}
