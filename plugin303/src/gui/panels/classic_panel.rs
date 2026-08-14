use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{CornerRadius, Frame, Margin, Ui};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::{CircuitTopology, Plugin303Params};

pub fn render_classic(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.set_min_height(160.0);
            ui.set_min_width(430.0);

            ui.label(
                nih_plug_egui::egui::RichText::new("CLASSIC 303 FRONT PANEL")
                    .font(nih_plug_egui::egui::FontId::new(
                        11.0,
                        nih_plug_egui::egui::FontFamily::Proportional,
                    ))
                    .strong()
                    .color(theme::TEXT_SECONDARY),
            );

            ui.add_space(12.0);

            // Row of main dials (canonical 7 vintage 303 rotary dials)
            ui.horizontal(|ui| {
                ui.add(Knob303::new(&params.tuning, setter, "TUNING", 15.0));
                ui.add(Knob303::new(&params.cutoff, setter, "CUTOFF", 15.0));
                ui.add(Knob303::new(&params.resonance, setter, "RESONANCE", 15.0));
                ui.add(Knob303::new(&params.env_mod, setter, "ENV MOD", 15.0));
                ui.add(Knob303::new(&params.decay, setter, "DECAY", 15.0));
                ui.add(Knob303::new(&params.accent, setter, "ACCENT", 15.0));
                ui.add(Knob303::new(&params.volume, setter, "VOLUME", 15.0));
            });

            ui.add_space(10.0);

            // Switches: Waveform & Tier
            ui.horizontal(|ui| {
                ui.add(SlideSwitch::new(
                    &params.waveform,
                    setter,
                    "WAVEFORM",
                    &["SAW", "SQUARE", "SWEEP"],
                ));
                ui.add_space(20.0);
                let is_accurate = params.topology.value() == CircuitTopology::Accurate;
                ui.add_enabled_ui(!is_accurate, |ui| {
                    ui.add(
                        SlideSwitch::new(
                            &params.tier,
                            setter,
                            "FIDELITY TIER",
                            &["LOW", "HIGH", "ULTRA"],
                        )
                        .with_thumb_color(theme::PLAYHEAD_RED),
                    );
                    if is_accurate {
                        ui.label(
                            nih_plug_egui::egui::RichText::new("(Bypassed)")
                                .font(nih_plug_egui::egui::FontId::new(
                                    8.5,
                                    nih_plug_egui::egui::FontFamily::Proportional,
                                ))
                                .color(theme::TEXT_MUTED),
                        );
                    }
                });
            });
        });
}
