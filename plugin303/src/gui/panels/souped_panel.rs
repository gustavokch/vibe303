use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{pos2, vec2, Align2, Button, Color32, CornerRadius, FontId, Frame, Margin, Rect, Stroke, Ui};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, MiniFader, SlideSwitch};
use crate::params::{CircuitTopology, Plugin303Params};

pub fn render_souped_panel(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let is_accurate = params.topology.value() == CircuitTopology::Accurate;

    Frame::new()
        .fill(theme::PANEL_BG_SOUPED)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            let panel_rect_start = ui.cursor().min;

            ui.vertical(|ui| {
                // Header Bar
                ui.horizontal(|ui| {
                    ui.label(
                        nih_plug_egui::egui::RichText::new("SOUPED-UP ENGINE DRAWER")
                            .font(FontId::new(11.0, nih_plug_egui::egui::FontFamily::Proportional))
                            .strong()
                            .color(if is_accurate {
                                theme::TEXT_MUTED
                            } else {
                                theme::SOUPED_ORANGE
                            }),
                    );
                    if is_accurate {
                        ui.label(
                            nih_plug_egui::egui::RichText::new("(Bypassed in Accurate Mode)")
                                .font(FontId::new(10.0, nih_plug_egui::egui::FontFamily::Proportional))
                                .color(theme::TEXT_MUTED),
                        );
                    }
                });

                ui.add_space(4.0);

                // Main Controls (Disabled in Accurate Mode)
                ui.add_enabled_ui(!is_accurate, |ui| {
                    ui.horizontal(|ui| {
                        // Left Column: Toggles & Dials
                        ui.vertical(|ui| {
                            // Selectors Row
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = vec2(8.0, 0.0);

                                ui.add(
                                    SlideSwitch::new(
                                        &params.tier,
                                        setter,
                                        "FIDELITY TIER",
                                        &["LOW", "HIGH", "ULTRA"],
                                    )
                                    .with_thumb_color(theme::PLAYHEAD_RED),
                                );

                                ui.add_space(6.0);

                                ui.add(SlideSwitch::new(
                                    &params.vcf_taps,
                                    setter,
                                    "VCF TAPS",
                                    &["2-POLE", "3-POLE", "4-POLE"],
                                ));
                            });

                            ui.add_space(6.0);
                            ui.separator();
                            ui.add_space(4.0);

                            // Extended Rotary Dials
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
                                ui.add(Knob303::new(&params.vcf_drive, setter, "DISTORTION", 13.0));
                                ui.add(Knob303::new(&params.sub_vol, setter, "SUB VOL", 13.0));
                                ui.add(Knob303::new(&params.glide, setter, "GLIDE", 13.0));
                                ui.add(Knob303::new(&params.osc2_pitch, setter, "OSC2 PITCH", 13.0));
                                ui.add(Knob303::new(&params.reso_comp, setter, "RESO COMP", 13.0));
                                ui.add(Knob303::new(&params.osc2_vol, setter, "OSC2 MIX", 13.0));
                            });
                        });

                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(10.0);

                        // Right Column: MEG & VEG ADSR Vertical Mini-Faders
                        ui.horizontal(|ui| {
                            // MEG ADSR Group
                            ui.vertical(|ui| {
                                ui.label(
                                    nih_plug_egui::egui::RichText::new("MEG ADSR")
                                        .font(FontId::new(9.0, nih_plug_egui::egui::FontFamily::Proportional))
                                        .strong()
                                        .color(theme::TEXT_SECONDARY),
                                );
                                ui.add_space(2.0);
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                                    ui.add(MiniFader::new(&params.meg_attack, setter, "A", 36.0, theme::SOUPED_ORANGE));
                                    ui.add(MiniFader::new(&params.meg_decay_adj, setter, "D", 36.0, theme::SOUPED_ORANGE));
                                    ui.add(MiniFader::new(&params.meg_sustain, setter, "S", 36.0, theme::SOUPED_ORANGE));
                                    ui.add(MiniFader::new(&params.meg_release, setter, "R", 36.0, theme::SOUPED_ORANGE));
                                });
                            });

                            ui.add_space(6.0);

                            // VEG ADSR Group
                            ui.vertical(|ui| {
                                ui.label(
                                    nih_plug_egui::egui::RichText::new("VEG ADSR")
                                        .font(FontId::new(9.0, nih_plug_egui::egui::FontFamily::Proportional))
                                        .strong()
                                        .color(theme::TEXT_SECONDARY),
                                );
                                ui.add_space(2.0);
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                                    ui.add(MiniFader::new(&params.veg_attack, setter, "A", 36.0, theme::SOUPED_ORANGE));
                                    ui.add(MiniFader::new(&params.veg_decay_adj, setter, "D", 36.0, theme::SOUPED_ORANGE));
                                    ui.add(MiniFader::new(&params.veg_sustain, setter, "S", 36.0, theme::SOUPED_ORANGE));
                                    ui.add(MiniFader::new(&params.veg_release, setter, "R", 36.0, theme::SOUPED_ORANGE));
                                });
                            });
                        });
                    });
                });
            });

            // Accurate Mode Overlay Badge
            if is_accurate {
                let panel_rect_end = ui.cursor().min;
                let overlay_rect = Rect::from_min_max(
                    panel_rect_start,
                    pos2(panel_rect_end.x.max(panel_rect_start.x + 480.0), panel_rect_end.y + 100.0),
                );

                let painter = ui.painter();
                // Semi-transparent dimming ground
                painter.rect_filled(
                    overlay_rect,
                    4.0,
                    Color32::from_rgba_premultiplied(0x0a, 0x10, 0x1d, 0xb0),
                );

                // Centered Badge
                let center = overlay_rect.center();
                let badge_width = 300.0;
                let badge_height = 54.0;
                let badge_rect = Rect::from_center_size(center, vec2(badge_width, badge_height));

                painter.rect_filled(badge_rect, 6.0, Color32::from_rgb(0x13, 0x1b, 0x2e));
                painter.rect_stroke(
                    badge_rect,
                    6.0,
                    Stroke::new(1.0_f32, theme::PANEL_BORDER_HIGHLIGHT),
                    nih_plug_egui::egui::StrokeKind::Inside,
                );

                painter.text(
                    pos2(center.x, center.y - 12.0),
                    Align2::CENTER_CENTER,
                    "ACCURATE MODE ACTIVE",
                    FontId::new(10.5, nih_plug_egui::egui::FontFamily::Proportional),
                    theme::TEXT_PRIMARY,
                );

                painter.text(
                    pos2(center.x, center.y + 2.0),
                    Align2::CENTER_CENTER,
                    "Souped-up engine is bypassed",
                    FontId::new(8.5, nih_plug_egui::egui::FontFamily::Proportional),
                    theme::TEXT_MUTED,
                );

                // Quick Switch Button
                let btn_rect = Rect::from_center_size(pos2(center.x, center.y + 16.0), vec2(120.0, 16.0));
                let btn_resp = ui.put(
                    btn_rect,
                    Button::new(
                        nih_plug_egui::egui::RichText::new("SWITCH TO FUN")
                            .font(FontId::new(8.5, nih_plug_egui::egui::FontFamily::Proportional))
                            .color(theme::SOUPED_ORANGE),
                    )
                    .fill(Color32::from_rgb(0x1e, 0x29, 0x3b))
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
        });
}
