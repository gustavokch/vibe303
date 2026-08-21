use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    pos2, vec2, Align2, Button, Color32, ComboBox, CornerRadius, DragValue, FontId, Frame, Margin, Rect, Stroke, Ui,
};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, MiniFader, SlideSwitch};
use crate::params::{CircuitTopology, CompTolerance, Plugin303Params};

pub fn render_souped_panel(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let is_accurate = params.topology.value() == CircuitTopology::Accurate;

    let response = Frame::new()
        .fill(theme::PANEL_BG_SOUPED)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.set_min_width(1060.0);
            ui.vertical(|ui| {
                // Header Bar
                ui.horizontal(|ui| {
                    // The bypass state is carried by the overlay badge; no
                    // redundant parenthetical here.
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

                // Main Controls (Disabled in Accurate Mode)
                ui.add_enabled_ui(!is_accurate, |ui| {
                    ui.horizontal(|ui| {
                        // Left/Main Area: Engine & Analog Controls
                        ui.vertical(|ui| {
                            ui.set_width(762.0);

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

                                ui.add_space(6.0);

                                let mut adv_env = params.advanced_env.value();
                                if ui.checkbox(&mut adv_env, "ADV ENV").changed() {
                                    setter.begin_set_parameter(&params.advanced_env);
                                    setter.set_parameter(&params.advanced_env, adv_env);
                                    setter.end_set_parameter(&params.advanced_env);
                                }

                                ui.add_space(6.0);

                                ui.label(
                                    nih_plug_egui::egui::RichText::new("TOLERANCE:")
                                        .font(FontId::proportional(9.5))
                                        .color(theme::TEXT_SECONDARY),
                                );
                                let tol = params.tolerance.value();
                                ComboBox::from_id_salt("tolerance_combo")
                                    .selected_text(match tol {
                                        CompTolerance::Zero => "0%",
                                        CompTolerance::PointFive => "0.5%",
                                        CompTolerance::Two => "2%",
                                        CompTolerance::Five => "5%",
                                        CompTolerance::Ten => "10%",
                                    })
                                    .show_ui(ui, |ui| {
                                        if ui.selectable_label(tol == CompTolerance::Zero, "0%").clicked() {
                                            setter.begin_set_parameter(&params.tolerance);
                                            setter.set_parameter(&params.tolerance, CompTolerance::Zero);
                                            setter.end_set_parameter(&params.tolerance);
                                        }
                                        if ui.selectable_label(tol == CompTolerance::PointFive, "0.5%").clicked() {
                                            setter.begin_set_parameter(&params.tolerance);
                                            setter.set_parameter(&params.tolerance, CompTolerance::PointFive);
                                            setter.end_set_parameter(&params.tolerance);
                                        }
                                        if ui.selectable_label(tol == CompTolerance::Two, "2%").clicked() {
                                            setter.begin_set_parameter(&params.tolerance);
                                            setter.set_parameter(&params.tolerance, CompTolerance::Two);
                                            setter.end_set_parameter(&params.tolerance);
                                        }
                                        if ui.selectable_label(tol == CompTolerance::Five, "5%").clicked() {
                                            setter.begin_set_parameter(&params.tolerance);
                                            setter.set_parameter(&params.tolerance, CompTolerance::Five);
                                            setter.end_set_parameter(&params.tolerance);
                                        }
                                        if ui.selectable_label(tol == CompTolerance::Ten, "10%").clicked() {
                                            setter.begin_set_parameter(&params.tolerance);
                                            setter.set_parameter(&params.tolerance, CompTolerance::Ten);
                                            setter.end_set_parameter(&params.tolerance);
                                        }
                                    });

                                ui.add_space(6.0);

                                // Analog Component Group (comp_age, thermal_noise, comp_seed)
                                ui.label(
                                    nih_plug_egui::egui::RichText::new("AGE:")
                                        .font(FontId::proportional(9.5))
                                        .color(theme::TEXT_SECONDARY),
                                );
                                let mut age_val = params.comp_age.value();
                                if ui.add(DragValue::new(&mut age_val).range(0..=100)).changed() {
                                    setter.begin_set_parameter(&params.comp_age);
                                    setter.set_parameter(&params.comp_age, age_val);
                                    setter.end_set_parameter(&params.comp_age);
                                }

                                ui.add_space(4.0);

                                ui.label(
                                    nih_plug_egui::egui::RichText::new("NOISE:")
                                        .font(FontId::proportional(9.5))
                                        .color(theme::TEXT_SECONDARY),
                                );
                                let mut noise_val = params.thermal_noise.value();
                                if ui.add(DragValue::new(&mut noise_val).range(0..=100)).changed() {
                                    setter.begin_set_parameter(&params.thermal_noise);
                                    setter.set_parameter(&params.thermal_noise, noise_val);
                                    setter.end_set_parameter(&params.thermal_noise);
                                }

                                ui.add_space(4.0);

                                ui.label(
                                    nih_plug_egui::egui::RichText::new("SEED:")
                                        .font(FontId::proportional(9.5))
                                        .color(theme::TEXT_SECONDARY),
                                );
                                let mut seed_val = params.comp_seed.value();
                                if ui.add(DragValue::new(&mut seed_val).range(0..=9999)).changed() {
                                    setter.begin_set_parameter(&params.comp_seed);
                                    setter.set_parameter(&params.comp_seed, seed_val);
                                    setter.end_set_parameter(&params.comp_seed);
                                }
                            });

                            ui.add_space(2.0);
                            ui.separator();
                            ui.add_space(2.0);

                            // Row 2: Engine Rotary Dials (including osc1_pitch & sweep_pos)
                            // Spread evenly across the 762px drawer column.
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
                                ui.add(Knob303::new(&params.comp_drift, setter, "DRIFT", 11.0));
                                ui.add(Knob303::new(&params.recalibration_scale, setter, "RECAL", 11.0));
                            });
                        });

                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(6.0);

                        // Right Column: MEG & VEG ADSR Vertical Mini-Faders
                        ui.vertical(|ui| {
                            ui.set_width(286.0);
                            ui.horizontal(|ui| {
                                let bank_frame = Frame::new()
                                    .fill(Color32::from_rgb(0x1b, 0x1b, 0x1b))
                                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2a, 0x2e, 0x33)))
                                    .corner_radius(CornerRadius::same(3))
                                    .inner_margin(Margin::symmetric(6, 4));

                                // MEG ADSR Group
                                bank_frame.show(ui, |ui| {
                                    ui.vertical(|ui| {
                                        ui.label(
                                            nih_plug_egui::egui::RichText::new("MEG ADSR")
                                                .font(FontId::new(9.5, nih_plug_egui::egui::FontFamily::Proportional))
                                                .strong()
                                                .color(theme::TEXT_PRIMARY),
                                        );
                                        ui.add_space(2.0);
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
                                            ui.add(MiniFader::new(&params.meg_attack, setter, "A", 44.0, theme::SOUPED_ORANGE));
                                            ui.add(MiniFader::new(&params.meg_decay_adj, setter, "D", 44.0, theme::SOUPED_ORANGE));
                                            ui.add(MiniFader::new(&params.meg_sustain, setter, "S", 44.0, theme::SOUPED_ORANGE));
                                            ui.add(MiniFader::new(&params.meg_release, setter, "R", 44.0, theme::SOUPED_ORANGE));
                                        });
                                    });
                                });

                                ui.add_space(4.0);

                                // VEG ADSR Group
                                bank_frame.show(ui, |ui| {
                                    ui.vertical(|ui| {
                                        ui.label(
                                            nih_plug_egui::egui::RichText::new("VEG ADSR")
                                                .font(FontId::new(9.5, nih_plug_egui::egui::FontFamily::Proportional))
                                                .strong()
                                                .color(theme::TEXT_PRIMARY),
                                        );
                                        ui.add_space(2.0);
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
                                            ui.add(MiniFader::new(&params.veg_attack, setter, "A", 44.0, theme::SOUPED_ORANGE));
                                            ui.add(MiniFader::new(&params.veg_decay_adj, setter, "D", 44.0, theme::SOUPED_ORANGE));
                                            ui.add(MiniFader::new(&params.veg_sustain, setter, "S", 44.0, theme::SOUPED_ORANGE));
                                            ui.add(MiniFader::new(&params.veg_release, setter, "R", 44.0, theme::SOUPED_ORANGE));
                                        });
                                    });
                                });
                            });
                        });
                    });
                });
            });
        });

    // Accurate Mode Overlay Badge
    if is_accurate {
        let overlay_rect = response.response.rect;

        let painter = ui.painter();
        // 70% occlusion scrim — content ghosted to ~30% visibility (spec 4.1).
        // Neutral chassis tone (#16181b), not blue-tinted.
        painter.rect_filled(
            overlay_rect,
            4.0,
            Color32::from_rgba_premultiplied(0x16, 0x18, 0x1b, 0xb2),
        );

        // Centered Badge with Drop Shadow
        let center = overlay_rect.center();
        let badge_width = 240.0;
        let badge_height = 56.0;
        let badge_rect = Rect::from_center_size(center, vec2(badge_width, badge_height));

        // Soft drop shadow
        painter.rect_filled(
            badge_rect.translate(vec2(0.0, 3.0)),
            8.0,
            Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x80),
        );

        painter.rect_filled(badge_rect, 6.0, Color32::from_rgb(0x18, 0x18, 0x18));
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
            pos2(center.x, center.y + 0.0),
            Align2::CENTER_CENTER,
            "Souped-up engine is bypassed",
            FontId::new(8.5, nih_plug_egui::egui::FontFamily::Proportional),
            theme::TEXT_MUTED,
        );

        // Quick Switch Button
        let btn_rect = Rect::from_center_size(pos2(center.x, center.y + 14.0), vec2(110.0, 16.0));
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
