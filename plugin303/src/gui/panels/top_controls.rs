use nih_plug::prelude::{FloatParam, ParamSetter};
use nih_plug_egui::egui::{vec2, CornerRadius, FontId, Frame, Margin, Stroke, Ui};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, MiniFader, SlideSwitch};
use crate::params::Plugin303Params;

/// Envelope Generators:
/// - MEG ADSR: Modulation Envelope Generator (modulates VCF cutoff filter modulation)
/// - VEG ADSR: Voice / Volume Envelope Generator (modulates VCA amplitude)
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
        ui.vertical(|ui| {
            ui.label(
                nih_plug_egui::egui::RichText::new(title)
                    .font(FontId::new(
                        8.5,
                        nih_plug_egui::egui::FontFamily::Proportional,
                    ))
                    .strong()
                    .color(if is_active { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY }),
            );
            ui.add_space(1.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
                ui.add(MiniFader::new(
                    attack,
                    setter,
                    "A",
                    24.0,
                    accent,
                ));
                ui.add(MiniFader::new(
                    decay,
                    setter,
                    "D",
                    24.0,
                    accent,
                ));
                ui.add(MiniFader::new(
                    sustain,
                    setter,
                    "S",
                    24.0,
                    accent,
                ));
                ui.add(MiniFader::new(
                    release,
                    setter,
                    "R",
                    24.0,
                    accent,
                ));
            });
        });
    });
}

pub fn render_top_controls(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let desired_size = vec2(ui.available_width(), 95.0);
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
                    SlideSwitch::new(
                        &params.waveform,
                        setter,
                        "WAVE",
                        &["SAW", "SQR", "SWP"],
                    )
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

                // Envelopes Module Frame: MEG/VEG/ADV ENV distributed evenly
                // across the full remaining width so there is no dead band.
                let env_frame = Frame::new()
                    .fill(theme::WELL_BG)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER_DEFAULT))
                    .corner_radius(CornerRadius::same(3))
                    .inner_margin(Margin::symmetric(6, 4));

                env_frame.show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.vertical(|ui| {
                        ui.label(
                            nih_plug_egui::egui::RichText::new("ENVELOPES")
                                .font(FontId::new(9.0, nih_plug_egui::egui::FontFamily::Proportional))
                                .strong()
                                .color(theme::TEXT_PRIMARY),
                        );

                        ui.add_space(2.0);

                        let is_adv = params.advanced_env.value();
                        ui.horizontal(|ui| {
                            ui.columns(3, |cols| {
                                cols[0].vertical_centered(|ui| {
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
                                });

                                cols[1].vertical_centered(|ui| {
                                    ui.add_enabled_ui(is_adv, |ui| {
                                        render_adsr_bank(
                                            ui,
                                            "VEG ADSR (Voice Envelope)",
                                            &params.veg_attack,
                                            &params.veg_decay_adj,
                                            &params.veg_sustain,
                                            &params.veg_release,
                                            setter,
                                            is_adv,
                                        );
                                    });
                                });

                                cols[2].vertical_centered(|ui| {
                                    let mut adv_env = params.advanced_env.value();
                                    let adv_color = if adv_env {
                                        theme::ACCENT_ORANGE
                                    } else {
                                        theme::TEXT_SECONDARY
                                    };
                                    if ui
                                        .checkbox(
                                            &mut adv_env,
                                            nih_plug_egui::egui::RichText::new("ADV ENV")
                                                .font(FontId::proportional(8.5))
                                                .color(adv_color),
                                        )
                                        .changed()
                                    {
                                        setter.begin_set_parameter(&params.advanced_env);
                                        setter.set_parameter(&params.advanced_env, adv_env);
                                        setter.end_set_parameter(&params.advanced_env);
                                    }
                                });
                            });
                        });
                    });
                });
            });
        },
    );
}
