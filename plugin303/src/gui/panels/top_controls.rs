use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{vec2, CornerRadius, Frame, Margin, Ui};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::{CircuitTopology, Plugin303Params};

pub fn render_top_controls(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let is_accurate = params.topology.value() == CircuitTopology::Accurate;

    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_min_width(1060.0);

            ui.vertical(|ui| {
                // ROW 1: Large Parameter Gauges (Tuning, Cutoff, Resonance, Env Mod, Decay, Accent, Volume)
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(14.0, 0.0);
                    ui.add(Knob303::new(&params.tuning, setter, "TUNING", 22.0));
                    ui.add(Knob303::new(&params.cutoff, setter, "CUTOFF", 22.0));
                    ui.add(Knob303::new(&params.resonance, setter, "RESONANCE", 22.0));
                    ui.add(Knob303::new(&params.env_mod, setter, "ENV MOD", 22.0));
                    ui.add(Knob303::new(&params.decay, setter, "DECAY", 22.0));
                    ui.add(Knob303::new(&params.accent, setter, "ACCENT", 22.0));
                    ui.add(Knob303::new(&params.volume, setter, "VOLUME", 22.0));
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);

                // ROW 2: Toggles, Selectors & Secondary Dials
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(10.0, 0.0);

                    // Waveform Segmented Toggle
                    ui.add(SlideSwitch::new(
                        &params.waveform,
                        setter,
                        "WAVEFORM",
                        &["SAW", "SQUARE", "SWEEP"],
                    ));

                    ui.add_space(4.0);

                    // Fidelity Tier Segmented Toggle
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
                    });

                    ui.add_space(4.0);

                    // VCF Slope Filter Taps Selector
                    ui.add_enabled_ui(!is_accurate, |ui| {
                        ui.add(SlideSwitch::new(
                            &params.vcf_taps,
                            setter,
                            "VCF TAPS",
                            &["2-POLE", "3-POLE", "4-POLE"],
                        ));
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Secondary Dials Cluster
                    ui.add_enabled_ui(!is_accurate, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
                            ui.add(Knob303::new(&params.vcf_drive, setter, "DISTORTION", 14.0));
                            ui.add(Knob303::new(&params.sub_vol, setter, "SUB VOL", 14.0));
                            ui.add(Knob303::new(&params.meg_decay_adj, setter, "ENVELOPE", 14.0));
                            ui.add(Knob303::new(&params.glide, setter, "GLIDE", 14.0));
                            ui.add(Knob303::new(&params.osc2_pitch, setter, "OSC2 PITCH", 14.0));
                            ui.add(Knob303::new(&params.reso_comp, setter, "RESO COMP", 14.0));
                            ui.add(Knob303::new(&params.osc2_vol, setter, "OSC2 MIX", 14.0));
                        });
                    });
                });
            });
        });
}
