use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    vec2, Color32, CornerRadius, FontFamily, FontId, Frame, Margin, Ui,
};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, MiniFader, SlideSwitch};
use crate::params::{CircuitTopology, Plugin303Params};

pub fn render_souped(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let is_accurate = params.topology.value() == CircuitTopology::Accurate;

    Frame::new()
        .fill(theme::PANEL_BG_SOUPED)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.set_min_height(160.0);
            ui.set_min_width(580.0);

            // Header Row
            ui.horizontal(|ui| {
                ui.label(
                    nih_plug_egui::egui::RichText::new("SOUPED-UP ENGINE DRAWER")
                        .font(FontId::new(10.5, FontFamily::Proportional))
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
                            .font(FontId::new(8.5, FontFamily::Proportional))
                            .color(theme::TEXT_MUTED),
                    );
                }
            });

            ui.add_space(4.0);

            let content_response = ui.add_enabled_ui(!is_accurate, |ui| {
                // Controls layout (rendered normally in Fun mode, ghosted/disabled in Accurate mode)
                ui.horizontal(|ui| {
                    // Left Column: Dials & Slopes
                    ui.vertical(|ui| {
                        // Section 1: Dual Osc & Sub
                        ui.label(
                            nih_plug_egui::egui::RichText::new("OSCILLATOR")
                                .font(FontId::new(8.5, FontFamily::Proportional))
                                .strong()
                                .color(if is_accurate { theme::TEXT_MUTED } else { theme::TEXT_SECONDARY }),
                        );
                        ui.horizontal(|ui| {
                            ui.add(Knob303::new(&params.osc1_pitch, setter, "OSC1 DET", 12.0));
                            ui.add(Knob303::new(&params.osc2_pitch, setter, "OSC2 DET", 12.0));
                            ui.add(Knob303::new(&params.osc2_vol, setter, "OSC2 MIX", 12.0));
                            ui.add(Knob303::new(&params.osc1_pw, setter, "OSC1 PW", 12.0));
                            ui.add(Knob303::new(&params.sub_vol, setter, "SUB VOL", 12.0));
                        });

                        ui.add_space(3.0);

                        // Section 2: Filter Slopes, Drive & Analog Drift
                        ui.label(
                            nih_plug_egui::egui::RichText::new("FILTER & ANALOG DRIFT")
                                .font(FontId::new(8.5, FontFamily::Proportional))
                                .strong()
                                .color(if is_accurate { theme::TEXT_MUTED } else { theme::TEXT_SECONDARY }),
                        );
                        ui.horizontal(|ui| {
                            ui.add(SlideSwitch::new(
                                &params.vcf_taps,
                                setter,
                                "VCF TAPS",
                                &["2-POLE", "3-POLE", "4-POLE"],
                            ));
                            ui.add_space(4.0);
                            ui.add(Knob303::new(&params.vcf_drive, setter, "DRIVE", 11.5));
                            ui.add(Knob303::new(&params.glide, setter, "GLIDE", 11.5));
                            ui.add(Knob303::new(&params.comp_drift, setter, "DRIFT", 11.5));
                            ui.add(Knob303::new(&params.comp_age, setter, "AGE", 11.5));
                            ui.add(Knob303::new(&params.reso_comp, setter, "RESO COMP", 11.5));
                            ui.add_space(4.0);
                            ui.add(SlideSwitch::new(
                                &params.tolerance,
                                setter,
                                "TOLERANCE",
                                &["0%", "0.5%", "2%", "5%", "10%"],
                            ));
                        });
                    });

                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);

                    // Right Column: ADSR Mini-Faders
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            // MEG ADSR Group
                            ui.vertical(|ui| {
                                ui.label(
                                    nih_plug_egui::egui::RichText::new("MEG ADSR")
                                        .font(FontId::new(8.5, FontFamily::Proportional))
                                        .strong()
                                        .color(if is_accurate { theme::TEXT_MUTED } else { theme::SOUPED_ORANGE }),
                                );
                                ui.horizontal(|ui| {
                                    ui.add(MiniFader::new(
                                        &params.meg_attack,
                                        setter,
                                        "A",
                                        45.0,
                                        theme::SOUPED_ORANGE,
                                    ));
                                    ui.add(MiniFader::new(
                                        &params.meg_decay_adj,
                                        setter,
                                        "D",
                                        45.0,
                                        theme::SOUPED_ORANGE,
                                    ));
                                    ui.add(MiniFader::new(
                                        &params.meg_sustain,
                                        setter,
                                        "S",
                                        45.0,
                                        theme::SOUPED_ORANGE,
                                    ));
                                    ui.add(MiniFader::new(
                                        &params.meg_release,
                                        setter,
                                        "R",
                                        45.0,
                                        theme::SOUPED_ORANGE,
                                    ));
                                });
                            });

                            ui.add_space(4.0);

                            // VEG ADSR Group
                            ui.vertical(|ui| {
                                ui.label(
                                    nih_plug_egui::egui::RichText::new("VEG ADSR")
                                        .font(FontId::new(8.5, FontFamily::Proportional))
                                        .strong()
                                        .color(if is_accurate { theme::TEXT_MUTED } else { theme::SOUPED_ORANGE }),
                                );
                                ui.horizontal(|ui| {
                                    ui.add(MiniFader::new(
                                        &params.veg_attack,
                                        setter,
                                        "A",
                                        45.0,
                                        theme::SOUPED_ORANGE,
                                    ));
                                    ui.add(MiniFader::new(
                                        &params.veg_decay_adj,
                                        setter,
                                        "D",
                                        45.0,
                                        theme::SOUPED_ORANGE,
                                    ));
                                    ui.add(MiniFader::new(
                                        &params.veg_sustain,
                                        setter,
                                        "S",
                                        45.0,
                                        theme::SOUPED_ORANGE,
                                    ));
                                    ui.add(MiniFader::new(
                                        &params.veg_release,
                                        setter,
                                        "R",
                                        45.0,
                                        theme::SOUPED_ORANGE,
                                    ));
                                });
                            });
                        });

                        ui.add_space(2.0);

                        ui.horizontal(|ui| {
                            ui.add(SlideSwitch::new(
                                &params.vca_env_mode,
                                setter,
                                "VCA MODE",
                                &["GATE", "ENV"],
                            ));
                        });
                    });
                });
            });

            if is_accurate {
                // Dimmed / Ghosted Accurate Mode Overlay over controls
                let overlay_rect = content_response.response.rect;

                let banner_width = 380.0;
                let banner_height = 86.0;
                let banner_rect = nih_plug_egui::egui::Rect::from_center_size(
                    overlay_rect.center(),
                    vec2(banner_width, banner_height),
                );

                let btn_rect = nih_plug_egui::egui::Rect::from_center_size(
                    nih_plug_egui::egui::pos2(banner_rect.center().x, banner_rect.bottom() - 22.0),
                    vec2(190.0, 22.0),
                );
                let btn_resp = ui.allocate_rect(btn_rect, nih_plug_egui::egui::Sense::click());
                let btn_hovered = btn_resp.hovered();

                let painter = ui.painter();

                // 30% opacity dark overlay tint (preserves visibility of ghosted controls)
                painter.rect_filled(
                    overlay_rect,
                    4.0,
                    Color32::from_rgba_premultiplied(0x0e, 0x11, 0x16, 0x4d),
                );
                painter.rect_stroke(
                    overlay_rect,
                    4.0,
                    nih_plug_egui::egui::Stroke::new(1.0_f32, theme::PANEL_BORDER),
                    nih_plug_egui::egui::StrokeKind::Inside,
                );

                painter.rect_filled(
                    banner_rect,
                    6.0,
                    Color32::from_rgb(0x16, 0x1a, 0x22),
                );
                painter.rect_stroke(
                    banner_rect,
                    6.0,
                    nih_plug_egui::egui::Stroke::new(1.5_f32, theme::SOUPED_ORANGE),
                    nih_plug_egui::egui::StrokeKind::Inside,
                );

                painter.text(
                    nih_plug_egui::egui::pos2(banner_rect.center().x, banner_rect.top() + 16.0),
                    nih_plug_egui::egui::Align2::CENTER_CENTER,
                    "ACCURATE MODE ACTIVE — SOUPED-UP ENGINE BYPASSED",
                    FontId::new(10.5, FontFamily::Proportional),
                    theme::TEXT_PRIMARY,
                );

                painter.text(
                    nih_plug_egui::egui::pos2(banner_rect.center().x, banner_rect.top() + 34.0),
                    nih_plug_egui::egui::Align2::CENTER_CENTER,
                    "Dual oscs, filter slopes, analog drift, and ADSR faders are bypassed in 1:1 emulation.",
                    FontId::new(8.5, FontFamily::Proportional),
                    theme::TEXT_MUTED,
                );

                painter.rect_filled(
                    btn_rect,
                    3.0,
                    if btn_hovered { Color32::from_rgb(0xff, 0x90, 0x28) } else { theme::SOUPED_ORANGE },
                );
                painter.text(
                    btn_rect.center(),
                    nih_plug_egui::egui::Align2::CENTER_CENTER,
                    "⚡ SWITCH TO FUN TOPOLOGY",
                    FontId::new(9.5, FontFamily::Proportional),
                    Color32::BLACK,
                );

                if btn_resp.clicked() {
                    setter.begin_set_parameter(&params.topology);
                    setter.set_parameter_normalized(&params.topology, 1.0);
                    setter.end_set_parameter(&params.topology);

                    setter.begin_set_parameter(&params.reso_comp);
                    setter.set_parameter_normalized(&params.reso_comp, 0.5);
                    setter.end_set_parameter(&params.reso_comp);
                }
            }
        });
}
