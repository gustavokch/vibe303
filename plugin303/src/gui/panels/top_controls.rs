use std::cell::Cell;

use nih_plug::prelude::{FloatParam, Param, ParamSetter};
use nih_plug_egui::egui::{vec2, Align, Layout, Ui};

use crate::gui::theme;
use crate::gui::widgets::caption::GROUP_CAPTION_H;
use crate::gui::widgets::panel::DevicePanel;
use crate::gui::widgets::toggle::ToggleButton;
use crate::gui::widgets::{fader, GroupCaption, Knob303, MiniFader, SlideSwitch};
use crate::params::{CircuitTopology, Plugin303Params};

/// Hover text of an ADSR fader while ADV ENV is off (round-20 #10).
pub const ADV_ENV_OFF_HINT: &str = "Turn on ADV ENV to edit the MEG and VEG envelopes.";
/// Hover text of the ADV ENV tile and the ADSR faders in ACCURATE.
pub const ADV_ENV_ACCURATE_HINT: &str =
    "ADV ENV applies to FUN mode only. Set TOPOLOGY to FUN to use it.";
/// Hover text of WAVE in FUN, where the voice ignores it (round-23 #1).
pub const WAVE_FUN_HINT: &str =
    "WAVE applies to ACCURATE mode. In FUN, the SWEEP knob sets the waveform.";
/// Hover text of the SWP segment in ACCURATE, which has no sweep wave.
pub const WAVE_SWP_HINT: &str =
    "ACCURATE has no sweep wave: SWP plays SAW. In FUN, the SWEEP knob morphs saw to square.";
/// Hover text of CUTOFF: what its Hz readout names.
pub const CUTOFF_HINT: &str = "Filter cutoff. The readout is the calibrated corner: ADV ENV \
                               changes its range, BRIGHT and AGE move the played corner around it.";
/// Index of SWP among the WAVE options.
const WAVE_SWP: usize = 2;

/// Envelope Generators:
/// - MEG ADSR: Modulation Envelope Generator (modulates VCF cutoff filter modulation)
/// - VEG ADSR: Voice / Volume Envelope Generator (modulates VCA amplitude)
fn render_adsr_bank(
    ui: &mut Ui,
    title: &str,
    faders: [(&FloatParam, &'static str); 4],
    track_h: f32,
    setter: &ParamSetter,
    disabled_hint: &str,
) {
    ui.vertical(|ui| {
        ui.add(GroupCaption::new(title));

        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let col_w = ui.available_width() / faders.len() as f32;
            let col_h = ui.available_height();
            for (param, label) in faders {
                ui.allocate_ui_with_layout(
                    vec2(col_w, col_h),
                    Layout::top_down(Align::Center),
                    |ui| {
                        ui.add(MiniFader::new(
                            param,
                            setter,
                            label,
                            track_h,
                            theme::ACCENT_ORANGE,
                        ))
                        .on_disabled_hover_text(disabled_hint);
                    },
                );
            }
        });
    });
}

pub fn render_voice_panel(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    DevicePanel::new("VOICE", crate::gui::VOICE_ROW_H).show(
        ui,
        |_| {},
        |body_ui| {
            body_ui.horizontal_top(|ui| {
                // The voice reads WAVE in ACCURATE only. FUN forces the sweep
                // oscillator, whose shape the SWEEP knob sets, and ACCURATE
                // plays SAW for SWP (`Voice::update_params`).
                let is_accurate = params.topology.value() == CircuitTopology::Accurate;
                ui.add_enabled_ui(is_accurate, |ui| {
                    ui.add(
                        SlideSwitch::new(&params.waveform, setter, "WAVE", &["SAW", "SQR", "SWP"])
                            .vertical()
                            .lock_option(WAVE_SWP, WAVE_SWP_HINT),
                    )
                    .on_disabled_hover_text(WAVE_FUN_HINT);
                });
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                let col_w = ui.available_width() / 7.0;
                let col_h = ui.available_height();
                let mut render_knob = |knob: Knob303<'_, _>| {
                    ui.allocate_ui_with_layout(
                        vec2(col_w, col_h),
                        Layout::top_down(Align::Center),
                        |ui| {
                            ui.add(knob);
                        },
                    );
                };
                render_knob(Knob303::new(
                    &params.tuning,
                    setter,
                    "TUNING",
                    theme::KNOB_RADIUS_LARGE,
                ));
                render_knob(
                    Knob303::new(&params.cutoff, setter, "CUTOFF", theme::KNOB_RADIUS_LARGE)
                        .value_text(crate::value_text::cutoff(
                            params.cutoff.unmodulated_plain_value(),
                            params.topology.value(),
                            params.advanced_env.value(),
                        ))
                        .tooltip(CUTOFF_HINT),
                );
                render_knob(Knob303::new(
                    &params.resonance,
                    setter,
                    "RESONANCE",
                    theme::KNOB_RADIUS_LARGE,
                ));
                render_knob(Knob303::new(
                    &params.env_mod,
                    setter,
                    "ENV MOD",
                    theme::KNOB_RADIUS_LARGE,
                ));
                render_knob(Knob303::new(
                    &params.decay,
                    setter,
                    "DECAY",
                    theme::KNOB_RADIUS_LARGE,
                ));
                render_knob(Knob303::new(
                    &params.accent,
                    setter,
                    "ACCENT",
                    theme::KNOB_RADIUS_LARGE,
                ));
                render_knob(Knob303::new(
                    &params.volume,
                    setter,
                    "VOLUME",
                    theme::KNOB_RADIUS_LARGE,
                ));
            });
        },
    );
}

pub fn render_envelope_panel(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let is_accurate = params.topology.value() == CircuitTopology::Accurate;
    // Header and body closures both live while the card is built, so the ADV ENV
    // state is shared through a Cell instead of a `&mut` the header would hold.
    let adv = Cell::new(params.advanced_env.value());
    DevicePanel::new("ENVELOPES", crate::gui::VOICE_ROW_H).show(
        ui,
        |header_ui| {
            header_ui.add_enabled_ui(!is_accurate, |ui| {
                let mut on = adv.get();
                if ui
                    .add(ToggleButton::new(&mut on, "ADV ENV"))
                    .on_hover_text("Lit: the MEG and VEG ADSR banks replace the classic 303 decay.")
                    .on_disabled_hover_text(ADV_ENV_ACCURATE_HINT)
                    .changed()
                {
                    adv.set(on);
                    setter.begin_set_parameter(&params.advanced_env);
                    setter.set_parameter(&params.advanced_env, on);
                    setter.end_set_parameter(&params.advanced_env);
                }
            });
        },
        |body_ui| {
            let banks_live = adv.get() && !is_accurate;
            let hint = if is_accurate {
                ADV_ENV_ACCURATE_HINT
            } else {
                ADV_ENV_OFF_HINT
            };
            let body_h = body_ui.available_height();
            body_ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                let track_h = (body_h - GROUP_CAPTION_H - fader::CHROME_H).max(10.0);

                let total_w = ui.available_width();
                // 13 px of inter-bank separator area: 6 px + 1 px separator + 6 px (<= 8 px each side)
                const SEP_PAD: f32 = 6.0;
                let bank_w = (total_w - (2.0 * SEP_PAD + 1.0)).max(0.0) * 0.5;
                let bank_h = body_h;

                ui.allocate_ui_with_layout(
                    vec2(bank_w, bank_h),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.add_enabled_ui(banks_live, |ui| {
                            render_adsr_bank(
                                ui,
                                "MEG ADSR (Mod Envelope)",
                                [
                                    (&params.meg_attack, "A"),
                                    (&params.meg_decay_adj, "D"),
                                    (&params.meg_sustain, "S"),
                                    (&params.meg_release, "R"),
                                ],
                                track_h,
                                setter,
                                hint,
                            );
                        });
                    },
                );

                ui.add_space(SEP_PAD);
                ui.separator();
                ui.add_space(SEP_PAD);

                ui.allocate_ui_with_layout(
                    vec2(bank_w, bank_h),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.add_enabled_ui(banks_live, |ui| {
                            render_adsr_bank(
                                ui,
                                "VEG ADSR (Voice Envelope)",
                                [
                                    (&params.veg_attack, "A"),
                                    (&params.veg_decay_adj, "D"),
                                    (&params.veg_sustain, "S"),
                                    (&params.veg_release, "R"),
                                ],
                                track_h,
                                setter,
                                hint,
                            );
                        });
                    },
                );
            });
        },
    );
}
