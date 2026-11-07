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
                ui.add(
                    SlideSwitch::new(&params.waveform, setter, "WAVE", &["SAW", "SQR", "SWP"])
                        .vertical(),
                );
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
                        )),
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
