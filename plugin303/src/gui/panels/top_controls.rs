use std::cell::Cell;

use nih_plug::prelude::{FloatParam, ParamSetter};
use nih_plug_egui::egui::{vec2, Align, FontFamily, FontId, Layout, Sense, Ui};

use crate::gui::theme;
use crate::gui::widgets::panel::DevicePanel;
use crate::gui::widgets::toggle::ToggleButton;
use crate::gui::widgets::{Knob303, MiniFader, SlideSwitch};
use crate::params::Plugin303Params;

const CAPTION_H: f32 = 11.0;

/// Envelope Generators:
/// - MEG ADSR: Modulation Envelope Generator (modulates VCF cutoff filter modulation)
/// - VEG ADSR: Voice / Volume Envelope Generator (modulates VCA amplitude)
fn render_adsr_bank(
    ui: &mut Ui,
    title: &str,
    faders: [(&FloatParam, &'static str); 4],
    track_h: f32,
    setter: &ParamSetter,
) {
    ui.vertical(|ui| {
        let is_enabled = ui.is_enabled();
        let color = if is_enabled {
            theme::TEXT_SECONDARY
        } else {
            theme::TEXT_MUTED
        };
        let font = FontId::new(8.5, FontFamily::Proportional);
        let galley_id = ui.id().with(("adsr_bank_title", title, color));
        let galley = ui
            .ctx()
            .data_mut(|d| d.get_temp::<std::sync::Arc<nih_plug_egui::egui::Galley>>(galley_id));
        let galley = match galley {
            Some(g) => g,
            None => {
                let g = ui.fonts(|f| f.layout_no_wrap(title.to_string(), font.clone(), color));
                ui.ctx().data_mut(|d| d.insert_temp(galley_id, g.clone()));
                g
            }
        };

        let (title_rect, _) =
            ui.allocate_exact_size(vec2(galley.size().x.ceil(), CAPTION_H), Sense::hover());
        let mut p = ui.painter().clone();
        p.set_fade_to_color(None);
        p.galley(title_rect.left_top(), galley, color);

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
                        ));
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
                render_knob(Knob303::new(
                    &params.cutoff,
                    setter,
                    "CUTOFF",
                    theme::KNOB_RADIUS_LARGE,
                ));
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
    // Header and body closures both live while the card is built, so the ADV ENV
    // state is shared through a Cell instead of a `&mut` the header would hold.
    let adv = Cell::new(params.advanced_env.value());
    DevicePanel::new("ENVELOPES", crate::gui::VOICE_ROW_H).show(
        ui,
        |header_ui| {
            let mut on = adv.get();
            if header_ui
                .add(ToggleButton::new(&mut on, "ADV ENV"))
                .changed()
            {
                adv.set(on);
                setter.begin_set_parameter(&params.advanced_env);
                setter.set_parameter(&params.advanced_env, on);
                setter.end_set_parameter(&params.advanced_env);
            }
        },
        |body_ui| {
            let body_h = body_ui.available_height();
            body_ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                let track_h = (body_h - CAPTION_H - 23.0).max(10.0);

                let total_w = ui.available_width();
                // 13 px of inter-bank separator area: 6 px + 1 px separator + 6 px (<= 8 px each side)
                const SEP_PAD: f32 = 6.0;
                let bank_w = (total_w - (2.0 * SEP_PAD + 1.0)).max(0.0) * 0.5;
                let bank_h = body_h;

                ui.allocate_ui_with_layout(
                    vec2(bank_w, bank_h),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.add_enabled_ui(adv.get(), |ui| {
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
                        );
                    },
                );
            });
        },
    );
}
