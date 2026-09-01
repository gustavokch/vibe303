use nih_plug::prelude::{FloatParam, ParamSetter};
use nih_plug_egui::egui::{vec2, CornerRadius, FontId, Frame, Margin, Stroke, Ui};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, MiniFader, SlideSwitch};
use crate::params::Plugin303Params;

/// Envelope Generators:
/// - MEG ADSR: Modulation Envelope Generator (modulates VCF cutoff filter modulation)
/// - VEG ADSR: Voice / Volume Envelope Generator (modulates VCA amplitude)
#[allow(clippy::too_many_arguments)]
fn render_adsr_bank(
    ui: &mut Ui,
    title: &str,
    attack: &FloatParam,
    decay: &FloatParam,
    sustain: &FloatParam,
    release: &FloatParam,
    setter: &ParamSetter,
    is_active: bool,
) {
    let accent = if is_active {
        theme::ACCENT_ORANGE
    } else {
        theme::TEXT_MUTED
    };

    let bank_frame = Frame::new()
        .fill(theme::WELL_BG)
        .stroke(Stroke::new(1.0_f32, theme::BORDER_DEFAULT))
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::symmetric(4, 2));

    bank_frame.show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        ui.vertical(|ui| {
            ui.label(
                nih_plug_egui::egui::RichText::new(title)
                    .font(FontId::new(
                        8.5,
                        nih_plug_egui::egui::FontFamily::Proportional,
                    ))
                    .strong()
                    .color(if is_active {
                        theme::TEXT_PRIMARY
                    } else {
                        theme::TEXT_SECONDARY
                    }),
            );
            ui.add_space(1.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
                ui.add(MiniFader::new(attack, setter, "A", 20.0, accent));
                ui.add(MiniFader::new(decay, setter, "D", 20.0, accent));
                ui.add(MiniFader::new(sustain, setter, "S", 20.0, accent));
                ui.add(MiniFader::new(release, setter, "R", 20.0, accent));
            });
        });
    });
}

pub fn render_top_controls(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    // 103 logical: knobs need 78; the ENVELOPES module (title row + two ADSR
    // banks of 41-px faders) needs ~84 and must fit inside this panel with
    // symmetric padding instead of overflowing it (round-11 item 2).
    let desired_size = vec2(ui.available_width(), 103.0);
    let (rect, _) = ui.allocate_exact_size(desired_size, nih_plug_egui::egui::Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(3), theme::PANEL_BG);
        painter.rect_stroke(
            rect,
            CornerRadius::same(3),
            theme::border_stroke(),
            nih_plug_egui::egui::StrokeKind::Inside,
        );
    }

    ui.allocate_new_ui(
        nih_plug_egui::egui::UiBuilder::new().max_rect(rect.shrink(6.0)),
        |ui| {
            ui.horizontal(|ui| {
                // Waveform Selector integrated inside main panel
                ui.add_space(4.0);
                ui.add(
                    SlideSwitch::new(&params.waveform, setter, "WAVE", &["SAW", "SQR", "SWP"])
                        .vertical()
                        .with_thumb_color(theme::ACCENT_ORANGE),
                );
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);

                // Classic 303 Parameter Gauges (7 Knobs)
                ui.spacing_mut().item_spacing = vec2(24.0, 0.0);
                ui.add(Knob303::new(&params.tuning, setter, "TUNING", 20.0));
                ui.add(Knob303::new(&params.cutoff, setter, "CUTOFF", 20.0));
                ui.add(Knob303::new(&params.resonance, setter, "RESONANCE", 20.0));
                ui.add(Knob303::new(&params.env_mod, setter, "ENV MOD", 20.0));
                ui.add(Knob303::new(&params.decay, setter, "DECAY", 20.0));
                ui.add(Knob303::new(&params.accent, setter, "ACCENT", 20.0));
                ui.add(Knob303::new(&params.volume, setter, "VOLUME", 20.0));

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(16.0);

                // Envelopes Module Frame: MEG/VEG/ADV ENV distributed cleanly
                // without isolated controls or dead gaps.
                let env_frame = Frame::new()
                    .fill(theme::WELL_BG)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER_DEFAULT))
                    .corner_radius(CornerRadius::same(3))
                    .inner_margin(Margin::symmetric(6, 4));

                env_frame.show(ui, |ui| {
                    // Compact envelope module: hugs its content (title row and
                    // the two ADSR banks) instead of stretching across the
                    // remaining panel width — kills the ~1150 px dead run
                    // between MEG and VEG (round-11 item 3).
                    ui.spacing_mut().item_spacing.y = 3.0;
                    ui.spacing_mut().interact_size.y = 14.0;
                    ui.vertical(|ui| {
                        let mut is_adv = params.advanced_env.value();
                        ui.horizontal(|ui| {
                            ui.label(
                                nih_plug_egui::egui::RichText::new("ENVELOPES")
                                    .font(FontId::new(
                                        9.0,
                                        nih_plug_egui::egui::FontFamily::Proportional,
                                    ))
                                    .strong()
                                    // Shared accent title colour (round-11 item 8).
                                    .color(theme::ACCENT_ORANGE),
                            );

                            ui.add_space(10.0);

                            let adv_color = if is_adv {
                                theme::ACCENT_ORANGE
                            } else {
                                theme::TEXT_SECONDARY
                            };
                            if ui
                                .checkbox(
                                    &mut is_adv,
                                    nih_plug_egui::egui::RichText::new("ADV ENV")
                                        .font(FontId::proportional(8.5))
                                        .color(adv_color),
                                )
                                .changed()
                            {
                                setter.begin_set_parameter(&params.advanced_env);
                                setter.set_parameter(&params.advanced_env, is_adv);
                                setter.end_set_parameter(&params.advanced_env);
                            }
                        });

                        ui.add_space(3.0);

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);

                            ui.add_enabled_ui(is_adv, |ui| {
                                render_adsr_bank(
                                    ui,
                                    "MEG ADSR (Mod Envelope)",
                                    &params.meg_attack,
                                    &params.meg_decay_adj,
                                    &params.meg_sustain,
                                    &params.meg_release,
                                    setter,
                                    is_adv,
                                );
                            });

                            ui.add_space(8.0);

                            // VEG (voice envelope) is live whether or not ADV
                            // ENV is engaged, so it must render enabled — the
                            // disabled tier is reserved for MEG (round-11 item 7).
                            render_adsr_bank(
                                ui,
                                "VEG ADSR (Voice Envelope)",
                                &params.veg_attack,
                                &params.veg_decay_adj,
                                &params.veg_sustain,
                                &params.veg_release,
                                setter,
                                true,
                            );
                        });
                    });
                });
            });
        },
    );
}
