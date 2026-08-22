use nih_plug::prelude::{FloatParam, ParamSetter};
use nih_plug_egui::egui::{vec2, CornerRadius, FontId, Frame, Margin, Stroke, Ui};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, MiniFader};
use crate::params::Plugin303Params;

fn render_adsr_bank(
    ui: &mut Ui,
    title: &str,
    attack: &FloatParam,
    decay: &FloatParam,
    sustain: &FloatParam,
    release: &FloatParam,
    setter: &ParamSetter,
) {
    let bank_frame = Frame::new()
        .fill(theme::WELL_BG)
        .stroke(Stroke::new(1.0_f32, theme::BORDER_DEFAULT))
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::symmetric(4, 3));

    bank_frame.show(ui, |ui| {
        ui.vertical(|ui| {
            ui.label(
                nih_plug_egui::egui::RichText::new(title)
                    .font(FontId::new(
                        8.5,
                        nih_plug_egui::egui::FontFamily::Proportional,
                    ))
                    .strong()
                    .color(theme::TEXT_PRIMARY),
            );
            ui.add_space(1.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
                ui.add(MiniFader::new(
                    attack,
                    setter,
                    "A",
                    32.0,
                    theme::ACCENT_ORANGE,
                ));
                ui.add(MiniFader::new(
                    decay,
                    setter,
                    "D",
                    32.0,
                    theme::ACCENT_ORANGE,
                ));
                ui.add(MiniFader::new(
                    sustain,
                    setter,
                    "S",
                    32.0,
                    theme::ACCENT_ORANGE,
                ));
                ui.add(MiniFader::new(
                    release,
                    setter,
                    "R",
                    32.0,
                    theme::ACCENT_ORANGE,
                ));
            });
        });
    });
}

pub fn render_top_controls(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let desired_size = vec2(ui.available_width(), 108.0);
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
                ui.add_space(8.0);

                // Classic 303 Parameter Gauges (7 Knobs)
                ui.spacing_mut().item_spacing = vec2(20.0, 0.0);
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

                // Envelopes Module Frame: contains ADV ENV toggle in header + MEG ADSR + VEG ADSR
                let env_frame = Frame::new()
                    .fill(theme::WELL_BG)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER_DEFAULT))
                    .corner_radius(CornerRadius::same(3))
                    .inner_margin(Margin::symmetric(6, 4));

                env_frame.show(ui, |ui| {
                    ui.vertical(|ui| {
                        // Header with Title and ADV ENV checkbox anchored together
                        ui.horizontal(|ui| {
                            ui.label(
                                nih_plug_egui::egui::RichText::new("ENVELOPES")
                                    .font(FontId::new(9.0, nih_plug_egui::egui::FontFamily::Proportional))
                                    .strong()
                                    .color(theme::TEXT_PRIMARY),
                            );
                            ui.add_space(8.0);
                            let mut adv_env = params.advanced_env.value();
                            if ui
                                .checkbox(
                                    &mut adv_env,
                                    nih_plug_egui::egui::RichText::new("ADV ENV")
                                        .font(FontId::proportional(8.5))
                                        .color(theme::TEXT_SECONDARY),
                                )
                                .changed()
                            {
                                setter.begin_set_parameter(&params.advanced_env);
                                setter.set_parameter(&params.advanced_env, adv_env);
                                setter.end_set_parameter(&params.advanced_env);
                            }
                        });

                        ui.add_space(2.0);

                        // ADSR banks row
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                            render_adsr_bank(
                                ui,
                                "MEG ADSR",
                                &params.meg_attack,
                                &params.meg_decay_adj,
                                &params.meg_sustain,
                                &params.meg_release,
                                setter,
                            );

                            render_adsr_bank(
                                ui,
                                "VEG ADSR",
                                &params.veg_attack,
                                &params.veg_decay_adj,
                                &params.veg_sustain,
                                &params.veg_release,
                                setter,
                            );
                        });
                    });
                });
            });
        },
    );
}
