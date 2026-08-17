use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{vec2, Color32, CornerRadius, FontId, Frame, Margin, Sense, Stroke, Ui};
use std::sync::atomic::{AtomicU8, Ordering};

use crate::gui::theme;
use crate::gui::widgets::piano_roll_popup::render_piano_roll_popup;
use crate::gui::widgets::render_step_strip;
use crate::params::Plugin303Params;

pub fn render_sequencer_and_transport(
    ui: &mut Ui,
    params: &Plugin303Params,
    gen_params: &mut crate::dsp::generator::GeneratorParams,
    setter: &ParamSetter,
    current_step_atomic: &AtomicU8,
) {
    let active_step = current_step_atomic.load(Ordering::Relaxed);

    ui.horizontal(|ui| {
        super::generator_panel::render_generator_panel(ui, params, gen_params, setter);
    });
    ui.add_space(4.0);

    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin { left: 10, right: 10, top: 4, bottom: 8 })
        .show(ui, |ui| {
            ui.set_min_width(1060.0);

            ui.horizontal(|ui| {
                // 1. LEFT / MAIN: 16-Step Pitch Key Columns
                ui.vertical(|ui| {
                    if let Ok(mut pattern_guard) = params.pattern.write() {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(4.0, 0.0);

                            for step_idx in 0..16 {
                                let is_current = active_step as usize == step_idx;
                                let step = &mut pattern_guard.steps[step_idx];

                                render_step_strip(ui, step_idx, step, is_current);
                            }
                        });

                        render_piano_roll_popup(ui.ctx(), &mut pattern_guard);
                    }
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);

                // 2. RIGHT COLUMN: Transport & Tempo Controls
                ui.vertical(|ui| {
                    ui.set_width(220.0);

                    ui.label(
                        nih_plug_egui::egui::RichText::new("TRANSPORT & MASTER")
                            .font(FontId::proportional(11.0))
                            .strong()
                            .color(theme::TEXT_PRIMARY),
                    );

                    ui.add_space(8.0);

                    // Large Run/Sync Button
                    let run_val = params.seq_run.value();
                    let run_btn = nih_plug_egui::egui::Button::new(
                        nih_plug_egui::egui::RichText::new(if run_val { "■ STOP" } else { "▶ RUN / SYNC" })
                            .font(FontId::proportional(14.0))
                            .strong()
                            .color(Color32::BLACK),
                    )
                    .fill(if run_val { theme::PLAYHEAD_RED } else { theme::SOUPED_ORANGE })
                    .min_size(vec2(200.0, 48.0));

                    if ui.add(run_btn).clicked() {
                        setter.begin_set_parameter(&params.seq_run);
                        setter.set_parameter(&params.seq_run, !run_val);
                        setter.end_set_parameter(&params.seq_run);
                    }

                    ui.add_space(8.0);

                    // Tempo Value Adjustment Box
                    ui.label(
                        nih_plug_egui::egui::RichText::new("TEMPO (BPM)")
                            .font(FontId::proportional(9.5))
                            .color(theme::TEXT_SECONDARY),
                    );

                    let tempo_val = params.seq_tempo.value();
                    ui.horizontal(|ui| {
                        let dn_btn = nih_plug_egui::egui::Button::new(
                            nih_plug_egui::egui::RichText::new("-")
                                .font(FontId::proportional(16.0))
                                .strong()
                                .color(theme::TEXT_PRIMARY),
                        )
                        .fill(Color32::from_rgb(0x1e, 0x29, 0x3b))
                        .min_size(vec2(36.0, 36.0));

                        if ui.add(dn_btn).clicked() {
                            setter.begin_set_parameter(&params.seq_tempo);
                            setter.set_parameter(&params.seq_tempo, (tempo_val - 1.0).clamp(40.0, 300.0));
                            setter.end_set_parameter(&params.seq_tempo);
                        }

                        let (tempo_rect, _) = ui.allocate_exact_size(vec2(110.0, 36.0), Sense::hover());
                        if ui.is_rect_visible(tempo_rect) {
                            let p = ui.painter();
                            p.rect_filled(tempo_rect, 4.0, Color32::from_rgb(0x0f, 0x17, 0x2a));
                            p.rect_stroke(
                                tempo_rect,
                                4.0,
                                Stroke::new(1.0_f32, theme::PANEL_BORDER),
                                nih_plug_egui::egui::StrokeKind::Inside,
                            );
                            p.text(
                                tempo_rect.center(),
                                nih_plug_egui::egui::Align2::CENTER_CENTER,
                                format!("{:.1}", tempo_val),
                                FontId::monospace(15.0),
                                theme::SOUPED_ORANGE,
                            );
                        }

                        let up_btn = nih_plug_egui::egui::Button::new(
                            nih_plug_egui::egui::RichText::new("+")
                                .font(FontId::proportional(16.0))
                                .strong()
                                .color(theme::TEXT_PRIMARY),
                        )
                        .fill(Color32::from_rgb(0x1e, 0x29, 0x3b))
                        .min_size(vec2(36.0, 36.0));

                        if ui.add(up_btn).clicked() {
                            setter.begin_set_parameter(&params.seq_tempo);
                            setter.set_parameter(&params.seq_tempo, (tempo_val + 1.0).clamp(40.0, 300.0));
                            setter.end_set_parameter(&params.seq_tempo);
                        }
                    });
                });
            });
        });
}
