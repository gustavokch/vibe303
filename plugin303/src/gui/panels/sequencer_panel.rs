use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{pos2, vec2, Color32, CornerRadius, FontId, Frame, Margin, Sense, Stroke, Ui};
use std::sync::atomic::{AtomicU8, Ordering};

use crate::gui::theme;
use crate::gui::widgets::piano_roll_popup::render_piano_roll_popup;
use crate::params::{Plugin303Params, StepGate};

pub fn render_sequencer_and_transport(
    ui: &mut Ui,
    params: &Plugin303Params,
    setter: &ParamSetter,
    current_step_atomic: &AtomicU8,
) {
    let active_step = current_step_atomic.load(Ordering::Relaxed);

    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_min_width(1060.0);

            ui.horizontal(|ui| {
                // 1. LEFT / MAIN: 16-Step Pitch Key Columns & Sub-rows for ACC, SLD, OCT
                ui.vertical(|ui| {
                    if let Ok(mut pattern_guard) = params.pattern.write() {
                        let num_steps = params.seq_length.value() as usize;

                        // TOP: 16 Piano Key Step Indicators
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(4.0, 0.0);

                            for step_idx in 0..16 {
                                let is_current = active_step as usize == step_idx;
                                let in_active_range = step_idx < num_steps;
                                let step = &mut pattern_guard.steps[step_idx];

                                let key_width = 44.0;
                                let key_height = 110.0;
                                let (key_rect, key_resp) = ui.allocate_exact_size(
                                    vec2(key_width, key_height),
                                    Sense::click(),
                                );

                                if key_resp.clicked() && in_active_range {
                                    crate::gui::widgets::piano_roll_popup::toggle_open_state(
                                        ui.ctx(),
                                        step_idx,
                                        key_rect,
                                    );
                                }

                                if ui.is_rect_visible(key_rect) {
                                    let p = ui.painter();
                                    let is_hovered = key_resp.hovered();

                                    // Key background color
                                    let fill_color = if !in_active_range {
                                        Color32::from_rgb(0x0f, 0x14, 0x1f)
                                    } else if is_current {
                                        theme::ACCENT_AMBER
                                    } else if step.gate == StepGate::Rest {
                                        Color32::from_rgb(0x1e, 0x26, 0x36)
                                    } else if is_hovered {
                                        Color32::from_rgb(0x33, 0x41, 0x55)
                                    } else {
                                        Color32::from_rgb(0x1e, 0x29, 0x3b)
                                    };

                                    let stroke_color = if is_current {
                                        theme::ACCENT_AMBER_GLOW
                                    } else {
                                        theme::PANEL_BORDER
                                    };

                                    p.rect_filled(key_rect, 4.0, fill_color);
                                    p.rect_stroke(
                                        key_rect,
                                        4.0,
                                        Stroke::new(1.2_f32, stroke_color),
                                        nih_plug_egui::egui::StrokeKind::Inside,
                                    );

                                    // Step Number Badge (Top)
                                    p.text(
                                        pos2(key_rect.center().x, key_rect.top() + 10.0),
                                        nih_plug_egui::egui::Align2::CENTER_CENTER,
                                        format!("{:02}", step_idx + 1),
                                        FontId::monospace(9.0),
                                        if is_current { Color32::BLACK } else { theme::TEXT_MUTED },
                                    );

                                    // Note Name & Octave (Center)
                                    let note_names = [
                                        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B", "C",
                                    ];
                                    let note_str = note_names
                                        .get(step.semitone.clamp(0, 12) as usize)
                                        .unwrap_or(&"C");
                                    let oct_num = 3 + step.octave + if step.semitone == 12 { 1 } else { 0 };

                                    let pitch_display = match step.gate {
                                        StepGate::Note => format!("{}{}", note_str, oct_num),
                                        StepGate::Tie => "TIE".to_string(),
                                        StepGate::Rest => "---".to_string(),
                                    };

                                    p.text(
                                        pos2(key_rect.center().x, key_rect.center().y),
                                        nih_plug_egui::egui::Align2::CENTER_CENTER,
                                        pitch_display,
                                        FontId::monospace(11.0),
                                        if is_current { Color32::BLACK } else { theme::TEXT_PRIMARY },
                                    );

                                    // Gate toggle hint (Bottom)
                                    p.text(
                                        pos2(key_rect.center().x, key_rect.bottom() - 10.0),
                                        nih_plug_egui::egui::Align2::CENTER_CENTER,
                                        "• STEP",
                                        FontId::proportional(8.0),
                                        if is_current { Color32::BLACK } else { theme::TEXT_MUTED },
                                    );
                                }
                            }
                        });

                        ui.add_space(6.0);

                        // ROW: Accent Toggles
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                            for step_idx in 0..16 {
                                let step = &mut pattern_guard.steps[step_idx];
                                let (btn_rect, btn_resp) = ui.allocate_exact_size(vec2(44.0, 22.0), Sense::click());
                                if btn_resp.clicked() {
                                    step.accent = !step.accent;
                                }
                                if ui.is_rect_visible(btn_rect) {
                                    let p = ui.painter();
                                    let fill = if step.accent {
                                        theme::SOUPED_ORANGE
                                    } else {
                                        Color32::from_rgb(0x1e, 0x29, 0x3b)
                                    };
                                    p.rect_filled(btn_rect, 3.0, fill);
                                    p.text(
                                        btn_rect.center(),
                                        nih_plug_egui::egui::Align2::CENTER_CENTER,
                                        "ACC",
                                        FontId::proportional(8.5),
                                        if step.accent { Color32::BLACK } else { theme::TEXT_MUTED },
                                    );
                                }
                            }
                        });

                        ui.add_space(4.0);

                        // ROW: Slide Toggles
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                            for step_idx in 0..16 {
                                let step = &mut pattern_guard.steps[step_idx];
                                let (btn_rect, btn_resp) = ui.allocate_exact_size(vec2(44.0, 22.0), Sense::click());
                                if btn_resp.clicked() {
                                    step.slide = !step.slide;
                                }
                                if ui.is_rect_visible(btn_rect) {
                                    let p = ui.painter();
                                    let fill = if step.slide {
                                        theme::SLIDE_CYAN
                                    } else {
                                        Color32::from_rgb(0x1e, 0x29, 0x3b)
                                    };
                                    p.rect_filled(btn_rect, 3.0, fill);
                                    p.text(
                                        btn_rect.center(),
                                        nih_plug_egui::egui::Align2::CENTER_CENTER,
                                        "SLD",
                                        FontId::proportional(8.5),
                                        if step.slide { Color32::BLACK } else { theme::TEXT_MUTED },
                                    );
                                }
                            }
                        });

                        ui.add_space(4.0);

                        // ROW: Octave Selectors (-1 / 0 / +1)
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                            for step_idx in 0..16 {
                                let step = &mut pattern_guard.steps[step_idx];
                                let (btn_rect, btn_resp) = ui.allocate_exact_size(vec2(44.0, 22.0), Sense::click());
                                if btn_resp.clicked() {
                                    step.octave = if step.octave >= 1 { -1 } else { step.octave + 1 };
                                }
                                if ui.is_rect_visible(btn_rect) {
                                    let p = ui.painter();
                                    let oct_str = format!("{:+} OCT", step.octave);
                                    p.rect_filled(btn_rect, 3.0, Color32::from_rgb(0x1e, 0x29, 0x3b));
                                    p.text(
                                        btn_rect.center(),
                                        nih_plug_egui::egui::Align2::CENTER_CENTER,
                                        oct_str,
                                        FontId::proportional(8.0),
                                        if step.octave != 0 { theme::TEXT_PRIMARY } else { theme::TEXT_MUTED },
                                    );
                                }
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

                    ui.add_space(12.0);

                    // Large Run/Sync Button
                    let run_val = params.seq_run.value();
                    let run_btn = nih_plug_egui::egui::Button::new(
                        nih_plug_egui::egui::RichText::new(if run_val { "■ STOP" } else { "▶ RUN / SYNC" })
                            .font(FontId::proportional(14.0))
                            .strong()
                            .color(Color32::BLACK),
                    )
                    .fill(if run_val { theme::PLAYHEAD_RED } else { theme::SOUPED_ORANGE })
                    .min_size(vec2(200.0, 50.0));

                    if ui.add(run_btn).clicked() {
                        setter.begin_set_parameter(&params.seq_run);
                        setter.set_parameter(&params.seq_run, !run_val);
                        setter.end_set_parameter(&params.seq_run);
                    }

                    ui.add_space(16.0);

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
