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
            ui.set_min_width(520.0);

            // Header Row
            ui.horizontal(|ui| {
                ui.label(
                    nih_plug_egui::egui::RichText::new("SOUPED-UP ENGINE DRAWER")
                        .font(FontId::new(11.0, FontFamily::Proportional))
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
                            .font(FontId::new(9.0, FontFamily::Proportional))
                            .color(theme::TEXT_MUTED),
                    );
                }
            });

            ui.add_space(4.0);

            if is_accurate {
                // Dimmed / Ghosted Accurate Mode Overlay
                let available_rect = ui.available_rect_before_wrap();
                let painter = ui.painter();

                // 30% opacity background tint
                painter.rect_filled(
                    available_rect,
                    4.0,
                    Color32::from_rgba_premultiplied(0x10, 0x14, 0x1a, 0xc0),
                );

                ui.vertical_centered(|ui| {
                    ui.add_space(32.0);
                    ui.label(
                        nih_plug_egui::egui::RichText::new(
                            "ACCURATE MODE ACTIVE — SOUPED-UP ENGINE BYPASSED",
                        )
                        .font(FontId::new(11.0, FontFamily::Proportional))
                        .strong()
                        .color(theme::TEXT_SECONDARY),
                    );
                    ui.label(
                        nih_plug_egui::egui::RichText::new(
                            "Dual oscillators, custom filter slopes, analog drift, and ADSR faders are disabled in 1:1 Accurate emulation.",
                        )
                        .font(FontId::new(9.0, FontFamily::Proportional))
                        .color(theme::TEXT_MUTED),
                    );
                    ui.add_space(10.0);

                    let enable_btn = nih_plug_egui::egui::Button::new(
                        nih_plug_egui::egui::RichText::new("⚡ SWITCH TO FUN TOPOLOGY")
                            .font(FontId::new(10.0, FontFamily::Proportional))
                            .strong()
                            .color(Color32::BLACK),
                    )
                    .fill(theme::SOUPED_ORANGE)
                    .min_size(vec2(180.0, 24.0));

                    if ui.add(enable_btn).clicked() {
                        setter.begin_set_parameter(&params.topology);
                        setter.set_parameter_normalized(&params.topology, 1.0);
                        setter.end_set_parameter(&params.topology);

                        setter.begin_set_parameter(&params.reso_comp);
                        setter.set_parameter_normalized(&params.reso_comp, 0.5);
                        setter.end_set_parameter(&params.reso_comp);
                    }
                });
            } else {
                // Interactive Fun Mode Controls
                ui.horizontal(|ui| {
                    // Left Column: Dials & Slopes
                    ui.vertical(|ui| {
                        // Section 1: Dual Osc & Drive
                        ui.label(
                            nih_plug_egui::egui::RichText::new("OSCILLATOR & DRIVE")
                                .font(FontId::new(8.5, FontFamily::Proportional))
                                .strong()
                                .color(theme::TEXT_MUTED),
                        );
                        ui.horizontal(|ui| {
                            ui.add(Knob303::new(&params.osc1_pitch, setter, "OSC1 DET", 13.0));
                            ui.add(Knob303::new(&params.osc2_pitch, setter, "OSC2 DET", 13.0));
                            ui.add(Knob303::new(&params.osc2_vol, setter, "OSC2 MIX", 13.0));
                            ui.add(Knob303::new(&params.osc1_pw, setter, "OSC1 PW", 13.0));
                            ui.add(Knob303::new(&params.sub_vol, setter, "SUB VOL", 13.0));
                            ui.add(Knob303::new(&params.vcf_drive, setter, "DRIVE", 13.0));
                            ui.add(Knob303::new(&params.glide, setter, "GLIDE", 13.0));
                        });

                        ui.add_space(4.0);

                        // Section 2: Filter Slopes & Analog Wear
                        ui.label(
                            nih_plug_egui::egui::RichText::new("FILTER & ANALOG DRIFT")
                                .font(FontId::new(8.5, FontFamily::Proportional))
                                .strong()
                                .color(theme::TEXT_MUTED),
                        );
                        ui.horizontal(|ui| {
                            ui.add(SlideSwitch::new(
                                &params.vcf_taps,
                                setter,
                                "VCF TAPS",
                                &["2-POLE", "3-POLE", "4-POLE"],
                            ));
                            ui.add_space(6.0);
                            ui.add(Knob303::new(&params.comp_drift, setter, "DRIFT", 12.0));
                            ui.add(Knob303::new(&params.comp_age, setter, "AGE", 12.0));
                            ui.add(Knob303::new(&params.reso_comp, setter, "RESO COMP", 12.0));
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
                                        .color(theme::SOUPED_ORANGE),
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
                                        .color(theme::ACCENT_AMBER),
                                );
                                ui.horizontal(|ui| {
                                    ui.add(MiniFader::new(
                                        &params.veg_attack,
                                        setter,
                                        "A",
                                        45.0,
                                        theme::ACCENT_AMBER,
                                    ));
                                    ui.add(MiniFader::new(
                                        &params.veg_decay_adj,
                                        setter,
                                        "D",
                                        45.0,
                                        theme::ACCENT_AMBER,
                                    ));
                                    ui.add(MiniFader::new(
                                        &params.veg_sustain,
                                        setter,
                                        "S",
                                        45.0,
                                        theme::ACCENT_AMBER,
                                    ));
                                    ui.add(MiniFader::new(
                                        &params.veg_release,
                                        setter,
                                        "R",
                                        45.0,
                                        theme::ACCENT_AMBER,
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
            }
        });
}
