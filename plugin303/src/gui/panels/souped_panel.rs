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
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.set_max_width(1048.0);
            ui.set_min_height(86.0);
            ui.vertical(|ui| {
                // Header Bar
                ui.horizontal(|ui| {
                    ui.label(
                        nih_plug_egui::egui::RichText::new("SOUPED-UP ENGINE & ANALOG DRAWER")
                            .font(FontId::new(11.0, nih_plug_egui::egui::FontFamily::Proportional))
                            .strong()
                            .color(if is_accurate {
                                theme::TEXT_MUTED
                            } else {
                                theme::SOUPED_ORANGE
                            }),
                    );
                });

                ui.add_space(4.0);

                // Row 1: Selectors & Mode Toggles
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 0.0);

                    ui.add(SlideSwitch::new(
                        &params.vcf_taps,
                        setter,
                        "VCF TAPS",
                        &["2-POLE", "3-POLE", "4-POLE"],
                    ));

                    ui.add_space(6.0);

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

                // Row 2: Engine Rotary Dials (full width, no DRIFT/RECAL)
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(34.0, 0.0);
                    ui.add_space(18.0);
                    ui.add(Knob303::new(&params.vcf_drive, setter, "DRIVE", 11.0));
                    ui.add(Knob303::new(&params.sub_vol, setter, "SUB", 11.0));
                    ui.add(Knob303::new(&params.glide, setter, "GLIDE", 11.0));
                    ui.add(Knob303::new(&params.osc1_pitch, setter, "PITCH1", 11.0));
                    ui.add(Knob303::new(&params.osc2_pitch, setter, "PITCH2", 11.0));
                    ui.add(Knob303::new(&params.osc2_vol, setter, "MIX2", 11.0));
                    ui.add(Knob303::new(&params.osc1_pw, setter, "PW", 11.0));
                    ui.add(Knob303::new(&params.sweep_pos, setter, "SWEEP", 11.0));
                    ui.add(Knob303::new(&params.reso_comp, setter, "R-COMP", 11.0));
                });
            });
        });

    // Accurate Mode Overlay Badge
    if is_accurate {
        let overlay_rect = response.response.rect;

        let painter = ui.painter();
        painter.rect_filled(
            overlay_rect,
            4.0,
            Color32::from_rgba_premultiplied(0x12, 0x14, 0x18, 0xd0),
        );

        let center = overlay_rect.center();
        let badge_width = 260.0;
        let badge_height = 54.0;
        let badge_rect = Rect::from_center_size(center, vec2(badge_width, badge_height));

        painter.rect_filled(
            badge_rect.translate(vec2(0.0, 3.0)),
            8.0,
            Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x80),
        );

        painter.rect_filled(badge_rect, 6.0, Color32::from_rgb(0x1c, 0x20, 0x26));
        painter.rect_stroke(
            badge_rect,
            6.0,
            Stroke::new(1.0_f32, theme::PANEL_BORDER_HIGHLIGHT),
            nih_plug_egui::egui::StrokeKind::Inside,
        );

        painter.text(
            pos2(center.x, center.y - 13.0),
            Align2::CENTER_CENTER,
            "ACCURATE MODE ACTIVE",
            FontId::new(10.5, nih_plug_egui::egui::FontFamily::Proportional),
            theme::TEXT_PRIMARY,
        );

        painter.text(
            pos2(center.x, center.y - 0.5),
            Align2::CENTER_CENTER,
            "Souped-up engine is bypassed",
            FontId::new(9.0, nih_plug_egui::egui::FontFamily::Proportional),
            theme::TEXT_SECONDARY,
        );

        let btn_rect = Rect::from_center_size(pos2(center.x, center.y + 14.5), vec2(110.0, 16.0));
        let btn_resp = ui.put(
            btn_rect,
            Button::new(
                nih_plug_egui::egui::RichText::new("SWITCH TO FUN")
                    .font(FontId::new(8.5, nih_plug_egui::egui::FontFamily::Proportional))
                    .color(theme::SOUPED_ORANGE),
            )
            .fill(Color32::from_rgb(0x28, 0x28, 0x28))
            .corner_radius(CornerRadius::same(3)),
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
