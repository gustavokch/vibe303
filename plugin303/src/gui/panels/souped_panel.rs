use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{CornerRadius, Frame, Margin, Ui};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, SlideSwitch};
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
            ui.set_min_width(450.0);

            ui.horizontal_wrapped(|ui| {
                ui.label(
                    nih_plug_egui::egui::RichText::new("SOUPED-UP ENGINE DRAWER")
                        .font(nih_plug_egui::egui::FontId::new(
                            11.0,
                            nih_plug_egui::egui::FontFamily::Proportional,
                        ))
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
                            .font(nih_plug_egui::egui::FontId::new(
                                9.5,
                                nih_plug_egui::egui::FontFamily::Proportional,
                            ))
                            .color(theme::ACCENT_AMBER),
                    );
                }
            });

            ui.add_space(4.0);

            ui.add_enabled_ui(!is_accurate, |ui| {
                // Group 1: Dual Osc & Drive
                ui.horizontal(|ui| {
                    ui.add(Knob303::new(&params.osc1_pitch, setter, "OSC1 DET", 14.0));
                    ui.add(Knob303::new(&params.osc2_pitch, setter, "OSC2 DET", 14.0));
                    ui.add(Knob303::new(&params.osc2_vol, setter, "OSC2 MIX", 14.0));
                    ui.add(Knob303::new(&params.osc1_pw, setter, "OSC1 PW", 14.0));
                    ui.add(Knob303::new(&params.sub_vol, setter, "SUB VOL", 14.0));
                    ui.add(Knob303::new(&params.vcf_drive, setter, "DRIVE", 14.0));
                    ui.add(Knob303::new(&params.glide, setter, "GLIDE", 14.0));
                });

                ui.add_space(6.0);

                // Group 2: Analog Wear & Filter Taps
                ui.horizontal(|ui| {
                    ui.add(SlideSwitch::new(
                        &params.vcf_taps,
                        setter,
                        "VCF TAPS",
                        &["2-POLE", "3-POLE", "4-POLE"],
                    ));
                    ui.add_space(10.0);
                    ui.add(Knob303::new(&params.comp_drift, setter, "DRIFT", 13.0));
                    ui.add(Knob303::new(&params.comp_age, setter, "AGE", 13.0));
                    ui.add(SlideSwitch::new(
                        &params.tolerance,
                        setter,
                        "TOLERANCE",
                        &["0%", "0.5%", "2%", "5%", "10%"],
                    ));
                });
            });
        });
}
