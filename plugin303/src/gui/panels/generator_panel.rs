use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    vec2, Button, ComboBox, CornerRadius, DragValue, FontId, Frame, Margin, Stroke, Ui,
};

use crate::dsp::generator::{AcidGenerator, GeneratorParams, MusicalScale};
use crate::gui::theme;
use crate::params::{Plugin303Params, SequencerStep, StepGate};

pub fn render_generator_panel(
    ui: &mut Ui,
    params: &Plugin303Params,
    gen_params: &mut GeneratorParams,
    setter: &ParamSetter,
) {
    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                // Header Bar
                ui.horizontal(|ui| {
                    ui.label(
                        nih_plug_egui::egui::RichText::new("ACID EUCLIDEAN GENERATOR")
                            .font(FontId::proportional(11.0))
                            .strong()
                            .color(theme::ACCENT_ORANGE),
                    );

                    ui.with_layout(
                        nih_plug_egui::egui::Layout::right_to_left(nih_plug_egui::egui::Align::Center),
                        |ui| {
                            ui.add_space(4.0);

                            // New Seed Button
                            let new_seed_btn = Button::new(
                                nih_plug_egui::egui::RichText::new("NEW SEED")
                                    .font(FontId::proportional(8.5))
                                    .color(theme::TEXT_PRIMARY),
                            )
                            .fill(theme::SURFACE_ELEVATED)
                            .stroke(Stroke::new(1.0_f32, theme::BORDER_DEFAULT))
                            .corner_radius(CornerRadius::same(2));

                            if ui.add(new_seed_btn).clicked() {
                                gen_params.seed = gen_params.seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
                            }

                            ui.add_space(6.0);

                            // Seed Display Badge
                            ui.label(
                                nih_plug_egui::egui::RichText::new(format!(
                                    "SEED: {:016X}",
                                    gen_params.seed
                                ))
                                .font(FontId::monospace(8.5))
                                .color(theme::TEXT_MUTED),
                            );
                        },
                    );
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // 4-Column Technical Grid
                ui.horizontal(|ui| {
                    let total_w = ui.available_width();
                    let col_w = (total_w - 36.0) / 4.0;
                    ui.spacing_mut().item_spacing = vec2(12.0, 0.0);

                    // ==========================================
                    // COLUMN 1: PATTERN STRUCTURE & SCALE
                    // ==========================================
                    ui.vertical(|ui| {
                        ui.set_width(col_w);
                        ui.label(
                            nih_plug_egui::egui::RichText::new("1. PATTERN STRUCTURE")
                                .font(FontId::proportional(9.0))
                                .strong()
                                .color(theme::TEXT_SECONDARY),
                        );
                        ui.add_space(2.0);

                        ui.horizontal(|ui| {
                            ui.label(
                                nih_plug_egui::egui::RichText::new("ROOT:")
                                    .font(FontId::proportional(8.5))
                                    .color(theme::TEXT_MUTED),
                            );
                            const NOTE_NAMES: [&str; 12] = [
                                "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
                            ];
                            ComboBox::from_id_salt("gen_root_key")
                                .selected_text(NOTE_NAMES[(gen_params.root_note % 12) as usize])
                                .width(36.0)
                                .show_ui(ui, |ui| {
                                    for (idx, name) in NOTE_NAMES.iter().enumerate() {
                                        ui.selectable_value(&mut gen_params.root_note, idx as u8, *name);
                                    }
                                });

                            ui.label(
                                nih_plug_egui::egui::RichText::new("SCALE:")
                                    .font(FontId::proportional(8.5))
                                    .color(theme::TEXT_MUTED),
                            );
                            ComboBox::from_id_salt("gen_scale")
                                .selected_text(match gen_params.scale {
                                    MusicalScale::MinorPentatonic => "Pentatonic",
                                    MusicalScale::Dorian => "Dorian",
                                    MusicalScale::Phrygian => "Phrygian",
                                    MusicalScale::Aeolian => "Aeolian",
                                    MusicalScale::HarmonicMinor => "Harmonic",
                                    MusicalScale::Chromatic => "Chromatic",
                                })
                                .width(60.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut gen_params.scale,
                                        MusicalScale::MinorPentatonic,
                                        "Pentatonic",
                                    );
                                    ui.selectable_value(
                                        &mut gen_params.scale,
                                        MusicalScale::Dorian,
                                        "Dorian",
                                    );
                                    ui.selectable_value(
                                        &mut gen_params.scale,
                                        MusicalScale::Phrygian,
                                        "Phrygian",
                                    );
                                    ui.selectable_value(
                                        &mut gen_params.scale,
                                        MusicalScale::Aeolian,
                                        "Aeolian",
                                    );
                                    ui.selectable_value(
                                        &mut gen_params.scale,
                                        MusicalScale::HarmonicMinor,
                                        "Harmonic",
                                    );
                                    ui.selectable_value(
                                        &mut gen_params.scale,
                                        MusicalScale::Chromatic,
                                        "Chromatic",
                                    );
                                });
                        });

                        ui.add_space(2.0);

                        ui.horizontal(|ui| {
                            ui.label(
                                nih_plug_egui::egui::RichText::new("PULSES (k):")
                                    .font(FontId::proportional(8.5))
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.add(DragValue::new(&mut gen_params.rhythm_pulses).range(1..=16));

                            ui.label(
                                nih_plug_egui::egui::RichText::new("STEPS (n):")
                                    .font(FontId::proportional(8.5))
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.add(DragValue::new(&mut gen_params.rhythm_length).range(1..=16));
                        });

                        ui.add_space(2.0);

                        ui.horizontal(|ui| {
                            ui.label(
                                nih_plug_egui::egui::RichText::new("SHIFT:")
                                    .font(FontId::proportional(8.5))
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.add(DragValue::new(&mut gen_params.rhythm_shift).range(0..=15));

                            ui.label(
                                nih_plug_egui::egui::RichText::new("PITCH LEN:")
                                    .font(FontId::proportional(8.5))
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.add(DragValue::new(&mut gen_params.pitch_length).range(1..=16));
                        });
                    });

                    // ==========================================
                    // COLUMN 2: OCTAVE BEHAVIOR
                    // ==========================================
                    ui.vertical(|ui| {
                        ui.set_width(col_w);
                        ui.label(
                            nih_plug_egui::egui::RichText::new("2. OCTAVE BEHAVIOR")
                                .font(FontId::proportional(9.0))
                                .strong()
                                .color(theme::TEXT_SECONDARY),
                        );
                        ui.add_space(2.0);

                        ui.horizontal(|ui| {
                            ui.label(
                                nih_plug_egui::egui::RichText::new("OCT MIN/MAX:")
                                    .font(FontId::proportional(8.5))
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.add(DragValue::new(&mut gen_params.octave_min).range(-1..=1));
                            ui.add(DragValue::new(&mut gen_params.octave_max).range(-1..=1));
                        });

                        ui.add_space(2.0);

                        ui.horizontal(|ui| {
                            ui.label(
                                nih_plug_egui::egui::RichText::new("OCT UP %:")
                                    .font(FontId::proportional(8.5))
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.add(
                                DragValue::new(&mut gen_params.octave_up_prob)
                                    .speed(0.01)
                                    .range(0.0..=1.0),
                            );
                        });

                        ui.add_space(2.0);

                        ui.horizontal(|ui| {
                            ui.label(
                                nih_plug_egui::egui::RichText::new("OCT DN %:")
                                    .font(FontId::proportional(8.5))
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.add(
                                DragValue::new(&mut gen_params.octave_down_prob)
                                    .speed(0.01)
                                    .range(0.0..=1.0),
                            );
                        });
                    });

                    // ==========================================
                    // COLUMN 3: EVENT PROBABILITIES
                    // ==========================================
                    ui.vertical(|ui| {
                        ui.set_width(col_w);
                        ui.label(
                            nih_plug_egui::egui::RichText::new("3. PROBABILITIES")
                                .font(FontId::proportional(9.0))
                                .strong()
                                .color(theme::TEXT_SECONDARY),
                        );
                        ui.add_space(2.0);

                        ui.horizontal(|ui| {
                            ui.label(
                                nih_plug_egui::egui::RichText::new("REST %:")
                                    .font(FontId::proportional(8.5))
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.add(
                                DragValue::new(&mut gen_params.rest_probability)
                                    .speed(0.01)
                                    .range(0.0..=1.0),
                            );

                            ui.label(
                                nih_plug_egui::egui::RichText::new("ACCENT %:")
                                    .font(FontId::proportional(8.5))
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.add(
                                DragValue::new(&mut gen_params.accent_probability)
                                    .speed(0.01)
                                    .range(0.0..=1.0),
                            );
                        });

                        ui.add_space(2.0);

                        ui.horizontal(|ui| {
                            ui.label(
                                nih_plug_egui::egui::RichText::new("SLIDE %:")
                                    .font(FontId::proportional(8.5))
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.add(
                                DragValue::new(&mut gen_params.slide_probability)
                                    .speed(0.01)
                                    .range(0.0..=1.0),
                            );

                            ui.label(
                                nih_plug_egui::egui::RichText::new("ACC BIAS %:")
                                    .font(FontId::proportional(8.5))
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.add(
                                DragValue::new(&mut gen_params.accent_pitch_bias)
                                    .speed(0.01)
                                    .range(0.0..=1.0),
                            );
                        });

                        ui.add_space(2.0);

                        ui.checkbox(&mut gen_params.slide_chaining, "SLD CHAIN");
                    });

                    // ==========================================
                    // COLUMN 4: ACTIONS & VARIATIONS
                    // ==========================================
                    ui.vertical(|ui| {
                        ui.set_width(col_w);
                        ui.label(
                            nih_plug_egui::egui::RichText::new("4. GENERATION")
                                .font(FontId::proportional(9.0))
                                .strong()
                                .color(theme::TEXT_SECONDARY),
                        );
                        ui.add_space(2.0);

                        ui.horizontal(|ui| {
                            ui.label(
                                nih_plug_egui::egui::RichText::new("MUTATION %:")
                                    .font(FontId::proportional(8.5))
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.add(
                                DragValue::new(&mut gen_params.mutation_rate)
                                    .speed(0.01)
                                    .range(0.0..=1.0),
                            );
                        });

                        ui.add_space(4.0);

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(6.0, 0.0);

                            // GENERATE Flat Button
                            let gen_btn = Button::new(
                                nih_plug_egui::egui::RichText::new("GENERATE")
                                    .font(FontId::proportional(9.5))
                                    .strong()
                                    .color(theme::TEXT_ON_ACCENT),
                            )
                            .fill(theme::ACCENT_ORANGE)
                            .min_size(vec2(col_w * 0.48, 22.0))
                            .corner_radius(CornerRadius::same(2));

                            if ui.add(gen_btn).clicked() {
                                let pat = AcidGenerator::generate(gen_params);
                                if let Ok(mut seq_pat) = params.pattern.write() {
                                    for i in 0..16 {
                                        let key = pat.pitches[i].key;
                                        let octave = pat.pitches[i].octave;
                                        let accent = pat.pitches[i].accent;
                                        let slide = pat.pitches[i].slide;
                                        let gate = match pat.timing[i] {
                                            crate::dsp::sequencer::TimeEntry::Note => StepGate::Note,
                                            crate::dsp::sequencer::TimeEntry::Tie => StepGate::Tie,
                                            crate::dsp::sequencer::TimeEntry::Rest => StepGate::Rest,
                                        };

                                        seq_pat.steps[i] = SequencerStep {
                                            semitone: key,
                                            octave,
                                            gate,
                                            accent,
                                            slide,
                                            gate_len: 0.5,
                                            velocity: if accent { 1.0 } else { 0.7 },
                                        };
                                    }
                                }

                                setter.begin_set_parameter(&params.seq_length);
                                setter.set_parameter(&params.seq_length, gen_params.rhythm_length as i32);
                                setter.end_set_parameter(&params.seq_length);
                            }

                            // MUTATE Flat Button
                            let mutate_btn = Button::new(
                                nih_plug_egui::egui::RichText::new("MUTATE")
                                    .font(FontId::proportional(9.5))
                                    .strong()
                                    .color(theme::TEXT_ON_ACCENT),
                            )
                            .fill(theme::ACCENT_CYAN)
                            .min_size(vec2(col_w * 0.48, 22.0))
                            .corner_radius(CornerRadius::same(2));

                            if ui.add(mutate_btn).clicked() {
                                if let Ok(mut seq_pat) = params.pattern.write() {
                                    let mode = params.seq_mode.value();
                                    let num_steps = params.seq_length.value() as u8;
                                    let mut pat = seq_pat.compile(num_steps, mode);
                                    gen_params.seed = gen_params.seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
                                    AcidGenerator::mutate(&mut pat, gen_params, gen_params.seed);

                                    for i in 0..16 {
                                        seq_pat.steps[i].accent = pat.pitches[i].accent;
                                        seq_pat.steps[i].slide = pat.pitches[i].slide;
                                        seq_pat.steps[i].semitone = pat.pitches[i].key;
                                        seq_pat.steps[i].octave = pat.pitches[i].octave;
                                        seq_pat.steps[i].gate = match pat.timing[i] {
                                            crate::dsp::sequencer::TimeEntry::Note => StepGate::Note,
                                            crate::dsp::sequencer::TimeEntry::Tie => StepGate::Tie,
                                            crate::dsp::sequencer::TimeEntry::Rest => StepGate::Rest,
                                        };
                                    }
                                }
                            }
                        });
                    });
                });
            });
        });
}
