use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    pos2, vec2, Align2, Button, ComboBox, CornerRadius, DragValue, FontId, Frame, Margin, Sense,
    Stroke, Ui,
};

use crate::dsp::generator::{AcidGenerator, GeneratorParams, MusicalScale};
use crate::gui::theme;
use crate::params::{Plugin303Params, SequencerStep, StepGate};

/// Label/field grid geometry for the four numbered sub-sections (round-11
/// item 11): every label sits in a fixed-width slot and every numeric field
/// gets the same width, so columns of the same kind share a left edge and a
/// width across all rows.
const G_LBL_W: f32 = 58.0;
const G_LBL2_W: f32 = 52.0;
const G_FIELD_W: f32 = 44.0;
const G_COMBO_W: f32 = 60.0;
const G_ROW_H: f32 = 20.0;

/// Fixed-width label cell, baseline-aligned with the field row.
fn grid_label(ui: &mut Ui, text: &str, w: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(w, G_ROW_H), Sense::hover());
    ui.painter().text(
        pos2(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(8.5),
        theme::TEXT_MUTED,
    );
}

/// Fixed-width numeric field cell (font comes from the global
/// `drag_value_text_style` rule = monospace).
fn grid_drag<'a>(ui: &mut Ui, drag: DragValue<'a>) {
    ui.add_sized(vec2(G_FIELD_W, G_ROW_H), drag);
}

fn fmt_octave(v: f64) -> String {
    let iv = v as i32;
    if iv == 0 {
        "0".to_string()
    } else {
        format!("{:+}", iv)
    }
}

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
        .inner_margin(Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical(|ui| {
                // Header Bar
                ui.horizontal(|ui| {
                    ui.label(
                        nih_plug_egui::egui::RichText::new("ACID EUCLIDEAN GENERATOR")
                            .font(FontId::proportional(11.0))
                            .strong()
                            .color(theme::ACCENT_ORANGE),
                    );

                    // Measure the NEW SEED button (cached galley + padding +
                    // stroke) so the spacer lands it flush right even if the
                    // label or font changes (PR#61 review).
                    let galley_id = ui.id().with("new_seed_btn_galley");
                    let galley = ui.ctx().data_mut(|d| {
                        d.get_temp::<std::sync::Arc<nih_plug_egui::egui::Galley>>(galley_id)
                    });
                    let galley = match galley {
                        Some(g) => g,
                        None => {
                            let g = ui.fonts(|f| {
                                f.layout_no_wrap(
                                    "NEW SEED".to_string(),
                                    FontId::proportional(8.5),
                                    theme::TEXT_PRIMARY,
                                )
                            });
                            ui.ctx().data_mut(|d| d.insert_temp(galley_id, g.clone()));
                            g
                        }
                    };
                    let right_group_w =
                        galley.size().x.ceil() + 2.0 * ui.spacing().button_padding.x + 2.0;
                    let space_w = (ui.available_width() - right_group_w).max(0.0);
                    ui.add_space(space_w);

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
                });

                ui.add_space(2.0);
                ui.separator();
                ui.add_space(2.0);

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
                            grid_label(ui, "ROOT:", G_LBL_W);
                            const NOTE_NAMES: [&str; 12] = [
                                "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
                            ];
                            ComboBox::from_id_salt("gen_root_key")
                                .selected_text(NOTE_NAMES[(gen_params.root_note % 12) as usize])
                                .width(G_COMBO_W)
                                .show_ui(ui, |ui| {
                                    for (idx, name) in NOTE_NAMES.iter().enumerate() {
                                        ui.selectable_value(
                                            &mut gen_params.root_note,
                                            idx as u8,
                                            *name,
                                        );
                                    }
                                });

                            grid_label(ui, "SCALE:", G_LBL2_W);
                            ComboBox::from_id_salt("gen_scale")
                                .selected_text(match gen_params.scale {
                                    MusicalScale::MinorPentatonic => "Pentatonic",
                                    MusicalScale::Dorian => "Dorian",
                                    MusicalScale::Phrygian => "Phrygian",
                                    MusicalScale::Aeolian => "Aeolian",
                                    MusicalScale::HarmonicMinor => "Harmonic",
                                    MusicalScale::Chromatic => "Chromatic",
                                })
                                .width(G_COMBO_W)
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
                            grid_label(ui, "PULSES (k):", G_LBL_W);
                            grid_drag(
                                ui,
                                DragValue::new(&mut gen_params.rhythm_pulses)
                                    .range(1..=16)
                                    .custom_formatter(|v, _| format!("{:02}", v as u32)),
                            );

                            grid_label(ui, "STEPS (n):", G_LBL2_W);
                            grid_drag(
                                ui,
                                DragValue::new(&mut gen_params.rhythm_length)
                                    .range(1..=16)
                                    .custom_formatter(|v, _| format!("{:02}", v as u32)),
                            );
                        });

                        ui.add_space(2.0);

                        ui.horizontal(|ui| {
                            grid_label(ui, "SHIFT:", G_LBL_W);
                            grid_drag(
                                ui,
                                DragValue::new(&mut gen_params.rhythm_shift)
                                    .range(0..=15)
                                    .custom_formatter(|v, _| format!("{:02}", v as u32)),
                            );

                            grid_label(ui, "PITCH LEN:", G_LBL2_W);
                            grid_drag(
                                ui,
                                DragValue::new(&mut gen_params.pitch_length)
                                    .range(1..=16)
                                    .custom_formatter(|v, _| format!("{:02}", v as u32)),
                            );
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
                            grid_label(ui, "OCT RANGE:", G_LBL_W);
                            grid_drag(
                                ui,
                                DragValue::new(&mut gen_params.octave_min)
                                    .range(-2..=2)
                                    .prefix("MIN ")
                                    .custom_formatter(|v, _| fmt_octave(v)),
                            );
                            grid_drag(
                                ui,
                                DragValue::new(&mut gen_params.octave_max)
                                    .range(-2..=2)
                                    .prefix("MAX ")
                                    .custom_formatter(|v, _| fmt_octave(v)),
                            );
                        });

                        ui.add_space(2.0);

                        ui.horizontal(|ui| {
                            grid_label(ui, "OCT UP:", G_LBL_W);
                            grid_drag(
                                ui,
                                DragValue::new(&mut gen_params.octave_up_prob)
                                    .speed(0.01)
                                    .range(0.0..=1.0)
                                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                            );
                        });

                        ui.add_space(2.0);

                        ui.horizontal(|ui| {
                            grid_label(ui, "OCT DN:", G_LBL_W);
                            grid_drag(
                                ui,
                                DragValue::new(&mut gen_params.octave_down_prob)
                                    .speed(0.01)
                                    .range(0.0..=1.0)
                                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
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
                            grid_label(ui, "REST:", G_LBL_W);
                            grid_drag(
                                ui,
                                DragValue::new(&mut gen_params.rest_probability)
                                    .speed(0.01)
                                    .range(0.0..=1.0)
                                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                            );

                            grid_label(ui, "ACCENT:", G_LBL2_W);
                            grid_drag(
                                ui,
                                DragValue::new(&mut gen_params.accent_probability)
                                    .speed(0.01)
                                    .range(0.0..=1.0)
                                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                            );
                        });

                        ui.add_space(2.0);

                        ui.horizontal(|ui| {
                            grid_label(ui, "SLIDE:", G_LBL_W);
                            grid_drag(
                                ui,
                                DragValue::new(&mut gen_params.slide_probability)
                                    .speed(0.01)
                                    .range(0.0..=1.0)
                                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                            );

                            grid_label(ui, "ACC BIAS:", G_LBL2_W);
                            grid_drag(
                                ui,
                                DragValue::new(&mut gen_params.accent_pitch_bias)
                                    .speed(0.01)
                                    .range(0.0..=1.0)
                                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                            );
                        });

                        ui.add_space(2.0);

                        ui.checkbox(
                            &mut gen_params.slide_chaining,
                            nih_plug_egui::egui::RichText::new("SLD CHAIN")
                                .font(FontId::proportional(8.5))
                                .color(theme::TEXT_SECONDARY),
                        );
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
                            grid_label(ui, "MUTATION:", G_LBL_W);
                            grid_drag(
                                ui,
                                DragValue::new(&mut gen_params.mutation_rate)
                                    .speed(0.01)
                                    .range(0.0..=1.0)
                                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                            );
                        });

                        ui.add_space(2.0);

                        // Seed readout as a flush-left grid row with bordered field chrome (round-18 issue 8)
                        ui.horizontal(|ui| {
                            grid_label(ui, "SEED:", G_LBL_W);
                            let mut seed_val = (gen_params.seed % 10000) as i32;
                            ui.add_enabled_ui(false, |ui| {
                                ui.add_sized(
                                    vec2(G_FIELD_W, G_ROW_H),
                                    DragValue::new(&mut seed_val)
                                        .custom_formatter(|v, _| format!("{:04}", v as i32)),
                                );
                            });
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
                                            crate::dsp::sequencer::TimeEntry::Note => {
                                                StepGate::Note
                                            }
                                            crate::dsp::sequencer::TimeEntry::Tie => StepGate::Tie,
                                            crate::dsp::sequencer::TimeEntry::Rest => {
                                                StepGate::Rest
                                            }
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
                                setter.set_parameter(
                                    &params.seq_length,
                                    gen_params.rhythm_length as i32,
                                );
                                setter.end_set_parameter(&params.seq_length);
                            }

                            // MUTATE button: secondary hierarchy (outline/flat surface, round-18 issue 15)
                            let mutate_btn = Button::new(
                                nih_plug_egui::egui::RichText::new("MUTATE")
                                    .font(FontId::proportional(9.5))
                                    .strong()
                                    .color(theme::TEXT_PRIMARY),
                            )
                            .fill(theme::SURFACE_ELEVATED)
                            .stroke(Stroke::new(1.0_f32, theme::BORDER_DEFAULT))
                            .min_size(vec2(col_w * 0.48, 22.0))
                            .corner_radius(CornerRadius::same(2));

                            if ui.add(mutate_btn).clicked() {
                                if let Ok(mut seq_pat) = params.pattern.write() {
                                    let mode = params.seq_mode.value();
                                    let num_steps = params.seq_length.value() as u8;
                                    let mut pat = seq_pat.compile(num_steps, mode);
                                    // Independent Weyl sequence constant for mutation stream (distinct from NEW SEED)
                                    gen_params.seed =
                                        gen_params.seed.wrapping_add(0x517C_C1B7_2722_0A95);
                                    AcidGenerator::mutate(&mut pat, gen_params, gen_params.seed);

                                    for i in 0..16 {
                                        seq_pat.steps[i].accent = pat.pitches[i].accent;
                                        seq_pat.steps[i].slide = pat.pitches[i].slide;
                                        seq_pat.steps[i].semitone = pat.pitches[i].key;
                                        seq_pat.steps[i].octave = pat.pitches[i].octave;
                                        seq_pat.steps[i].gate = match pat.timing[i] {
                                            crate::dsp::sequencer::TimeEntry::Note => {
                                                StepGate::Note
                                            }
                                            crate::dsp::sequencer::TimeEntry::Tie => StepGate::Tie,
                                            crate::dsp::sequencer::TimeEntry::Rest => {
                                                StepGate::Rest
                                            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn octave_formatter_hides_signed_zero() {
        assert_eq!(fmt_octave(0.0), "0");
        assert_eq!(fmt_octave(1.0), "+1");
        assert_eq!(fmt_octave(-1.0), "-1");
        assert_eq!(fmt_octave(0.5), "0"); // truncates toward zero; range is integral
    }
}
