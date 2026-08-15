use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{vec2, Color32, ComboBox, DragValue, FontId, Frame, Margin, Ui};

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
        .corner_radius(nih_plug_egui::egui::CornerRadius::same(4))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.label(
                nih_plug_egui::egui::RichText::new("ACID EUCLIDEAN GENERATOR")
                    .font(FontId::proportional(11.0))
                    .strong()
                    .color(theme::SOUPED_ORANGE),
            );

            ui.add_space(6.0);

            // Row 1: Scale & Key Selection
            ui.horizontal(|ui| {
                ui.label(
                    nih_plug_egui::egui::RichText::new("Root Key:")
                        .font(FontId::proportional(10.0))
                        .color(theme::TEXT_SECONDARY),
                );

                const NOTE_NAMES: [&str; 12] = [
                    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
                ];
                ComboBox::from_id_salt("gen_root_key")
                    .selected_text(NOTE_NAMES[(gen_params.root_note % 12) as usize])
                    .show_ui(ui, |ui| {
                        for (idx, name) in NOTE_NAMES.iter().enumerate() {
                            ui.selectable_value(&mut gen_params.root_note, idx as u8, *name);
                        }
                    });

                ui.add_space(8.0);

                ui.label(
                    nih_plug_egui::egui::RichText::new("Scale:")
                        .font(FontId::proportional(10.0))
                        .color(theme::TEXT_SECONDARY),
                );

                ComboBox::from_id_salt("gen_scale")
                    .selected_text(match gen_params.scale {
                        MusicalScale::MinorPentatonic => "Minor Pentatonic",
                        MusicalScale::Dorian => "Dorian",
                        MusicalScale::Phrygian => "Phrygian",
                        MusicalScale::Aeolian => "Aeolian (Minor)",
                        MusicalScale::HarmonicMinor => "Harmonic Minor",
                        MusicalScale::Chromatic => "Chromatic",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut gen_params.scale, MusicalScale::MinorPentatonic, "Minor Pentatonic");
                        ui.selectable_value(&mut gen_params.scale, MusicalScale::Dorian, "Dorian");
                        ui.selectable_value(&mut gen_params.scale, MusicalScale::Phrygian, "Phrygian");
                        ui.selectable_value(&mut gen_params.scale, MusicalScale::Aeolian, "Aeolian (Minor)");
                        ui.selectable_value(&mut gen_params.scale, MusicalScale::HarmonicMinor, "Harmonic Minor");
                        ui.selectable_value(&mut gen_params.scale, MusicalScale::Chromatic, "Chromatic");
                    });
            });

            ui.add_space(6.0);

            // Row 2: Euclidean Pulses & Steps
            ui.horizontal(|ui| {
                ui.label(
                    nih_plug_egui::egui::RichText::new("Pulses (k):")
                        .font(FontId::proportional(10.0))
                        .color(theme::TEXT_SECONDARY),
                );
                ui.add(DragValue::new(&mut gen_params.rhythm_pulses).range(1..=16));

                ui.add_space(6.0);

                ui.label(
                    nih_plug_egui::egui::RichText::new("Steps (n):")
                        .font(FontId::proportional(10.0))
                        .color(theme::TEXT_SECONDARY),
                );
                ui.add(DragValue::new(&mut gen_params.rhythm_length).range(1..=16));

                ui.add_space(6.0);

                ui.label(
                    nih_plug_egui::egui::RichText::new("Shift:")
                        .font(FontId::proportional(10.0))
                        .color(theme::TEXT_SECONDARY),
                );
                ui.add(DragValue::new(&mut gen_params.rhythm_shift).range(0..=15));
            });

            ui.add_space(6.0);

            // Row 3: Probabilities (Accent %, Slide %, Octave %, Mutation %)
            ui.horizontal(|ui| {
                ui.label(
                    nih_plug_egui::egui::RichText::new("Accent %:")
                        .font(FontId::proportional(9.5))
                        .color(theme::TEXT_SECONDARY),
                );
                ui.add(DragValue::new(&mut gen_params.accent_probability).speed(0.01).range(0.0..=1.0));

                ui.add_space(4.0);

                ui.label(
                    nih_plug_egui::egui::RichText::new("Slide %:")
                        .font(FontId::proportional(9.5))
                        .color(theme::TEXT_SECONDARY),
                );
                ui.add(DragValue::new(&mut gen_params.slide_probability).speed(0.01).range(0.0..=1.0));

                ui.add_space(4.0);

                ui.label(
                    nih_plug_egui::egui::RichText::new("Mutation %:")
                        .font(FontId::proportional(9.5))
                        .color(theme::TEXT_SECONDARY),
                );
                ui.add(DragValue::new(&mut gen_params.mutation_rate).speed(0.01).range(0.0..=1.0));
            });

            ui.add_space(8.0);

            // Row 4: Action Buttons [GENERATE] [MUTATE] [NEW SEED]
            ui.horizontal(|ui| {
                let gen_btn = nih_plug_egui::egui::Button::new(
                    nih_plug_egui::egui::RichText::new("🎲 GENERATE")
                        .font(FontId::proportional(11.0))
                        .strong()
                        .color(Color32::BLACK),
                )
                .fill(theme::SOUPED_ORANGE)
                .min_size(vec2(90.0, 26.0));

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

                ui.add_space(6.0);

                let mutate_btn = nih_plug_egui::egui::Button::new(
                    nih_plug_egui::egui::RichText::new("⚡ MUTATE")
                        .font(FontId::proportional(11.0))
                        .strong()
                        .color(theme::TEXT_PRIMARY),
                )
                .fill(Color32::from_rgb(0x33, 0x41, 0x55))
                .min_size(vec2(80.0, 26.0));

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

                ui.add_space(6.0);

                let new_seed_btn = nih_plug_egui::egui::Button::new(
                    nih_plug_egui::egui::RichText::new("NEW SEED")
                        .font(FontId::proportional(10.0))
                        .color(theme::TEXT_SECONDARY),
                )
                .fill(Color32::from_rgb(0x1e, 0x29, 0x3b))
                .min_size(vec2(70.0, 26.0));

                if ui.add(new_seed_btn).clicked() {
                    gen_params.seed = gen_params.seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
                }
            });
        });
}
