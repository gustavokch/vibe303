use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    pos2, vec2, Align, Align2, Button, ComboBox, DragValue, FontId, Layout, Response, Sense,
    Separator, Ui,
};

use crate::dsp::generator::{AcidGenerator, GeneratorParams, MusicalScale};
use crate::gui::theme;
use crate::gui::widgets::{DevicePanel, GroupCaption, Readout, ToggleButton};
use crate::params::{Plugin303Params, SequencerStep, StepGate};

/// Device height for both generator cards in the generator row.
pub const GENERATOR_H: f32 = 102.0;

const G_FIELD_W: f32 = 34.0;
/// SCALE well: the widest scale name with room to spare.
const G_SCALE_W: f32 = 84.0;
const G_ROW_H: f32 = 18.0;

const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// ROOT and SCALE wells: a narrow ▼ and 4 pt padding, so ROOT fits `C#` in a
/// numeric field's width and neither well grows with its text. A combo's
/// width is only a floor; ROOT grew with its text and moved SCALE (round-22 #7).
fn compact_combo_style(ui: &mut Ui) {
    let spacing = ui.spacing_mut();
    spacing.button_padding.x = 4.0;
    spacing.icon_width = 10.0;
    spacing.icon_spacing = 2.0;
}

/// Fixed-width label cell, vertically centered with the field row.
fn field_label(ui: &mut Ui, text: &str, w: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(w, G_ROW_H), Sense::hover());
    ui.painter().text(
        pos2(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(8.5),
        theme::TEXT_SECONDARY,
    );
}

/// Numeric field cell with configurable width.
fn field_drag<'a>(ui: &mut Ui, drag: DragValue<'a>, w: f32) -> Response {
    ui.add_sized(vec2(w, G_ROW_H), drag)
}

/// An octave offset with its sign, zero included, as the knobs show `+0.0`
/// (round-22 #16).
fn fmt_octave(v: f64) -> String {
    format!("{:+}", v as i32)
}

pub fn render_generator_panel(ui: &mut Ui, gen_params: &mut GeneratorParams) {
    DevicePanel::new("ACID EUCLIDEAN GENERATOR", GENERATOR_H).show(
        ui,
        |_| {},
        |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let sep_w = 6.0;
                let total_w = ui.available_width();
                let col_w = ((total_w - 2.0 * sep_w) / 3.0).floor();

                // ── Group 1: PATTERN STRUCTURE ──────────────────────────────
                ui.vertical(|ui| {
                    ui.set_width(col_w);
                    ui.spacing_mut().item_spacing = vec2(4.0, 3.0);

                    ui.add(GroupCaption::new("PATTERN STRUCTURE"));

                    ui.scope(|ui| {
                        theme::field_style(ui);
                        const G1_LBL_W: f32 = 46.0;

                        ui.horizontal(|ui| {
                            compact_combo_style(ui);
                            field_label(ui, "ROOT", G1_LBL_W);
                            // Tall enough for all twelve roots at once: egui's
                            // default (200 pt) hid A# and B (round-22 #3).
                            let root_rows_h = {
                                let s = &ui.ctx().style().spacing;
                                NOTE_NAMES.len() as f32 * (s.interact_size.y + s.item_spacing.y)
                            };
                            ComboBox::from_id_salt("gen_root_key")
                                .selected_text(NOTE_NAMES[(gen_params.root_note % 12) as usize])
                                .width(G_FIELD_W)
                                .height(root_rows_h)
                                .show_ui(ui, |ui| {
                                    for (idx, name) in NOTE_NAMES.iter().enumerate() {
                                        ui.selectable_value(
                                            &mut gen_params.root_note,
                                            idx as u8,
                                            *name,
                                        );
                                    }
                                });

                            field_label(ui, "SCALE", G1_LBL_W);
                            ComboBox::from_id_salt("gen_scale")
                                .selected_text(match gen_params.scale {
                                    MusicalScale::MinorPentatonic => "Pentatonic",
                                    MusicalScale::Dorian => "Dorian",
                                    MusicalScale::Phrygian => "Phrygian",
                                    MusicalScale::Aeolian => "Aeolian",
                                    MusicalScale::HarmonicMinor => "Harmonic",
                                    MusicalScale::Chromatic => "Chromatic",
                                })
                                .width(G_SCALE_W)
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

                        ui.horizontal(|ui| {
                            field_label(ui, "PULSES (k)", G1_LBL_W);
                            field_drag(
                                ui,
                                DragValue::new(&mut gen_params.rhythm_pulses)
                                    .range(1..=16)
                                    .custom_formatter(|v, _| format!("{:02}", v as u32)),
                                G_FIELD_W,
                            )
                            .on_hover_text("Euclidean pulses: notes spread evenly over STEPS");

                            field_label(ui, "STEPS (n)", G1_LBL_W);
                            field_drag(
                                ui,
                                DragValue::new(&mut gen_params.rhythm_length)
                                    .range(1..=16)
                                    .custom_formatter(|v, _| format!("{:02}", v as u32)),
                                G_FIELD_W,
                            )
                            .on_hover_text("Euclidean pattern length in steps");
                        });

                        ui.horizontal(|ui| {
                            field_label(ui, "SHIFT", G1_LBL_W);
                            field_drag(
                                ui,
                                DragValue::new(&mut gen_params.rhythm_shift)
                                    .range(0..=15)
                                    .custom_formatter(|v, _| format!("{:02}", v as u32)),
                                G_FIELD_W,
                            );

                            field_label(ui, "PITCH LEN", G1_LBL_W);
                            field_drag(
                                ui,
                                DragValue::new(&mut gen_params.pitch_length)
                                    .range(1..=16)
                                    .custom_formatter(|v, _| format!("{:02}", v as u32)),
                                G_FIELD_W,
                            )
                            .on_hover_text(
                                "Pitch sequence length. When it differs from STEPS, pitches and \
                                 rhythm repeat at different lengths.",
                            );
                        });
                    });
                });

                ui.add(Separator::default().vertical().spacing(sep_w));

                // ── Group 2: OCTAVE BEHAVIOR ────────────────────────────────
                ui.vertical(|ui| {
                    ui.set_width(col_w);
                    ui.spacing_mut().item_spacing = vec2(4.0, 3.0);

                    ui.add(GroupCaption::new("OCTAVE BEHAVIOR"));

                    if gen_params.octave_min > gen_params.octave_max {
                        // A preset or hand-edited state can load an inverted pair.
                        // Swap rather than let the ranges below collapse-and-widen
                        // both stored values on the first interaction.
                        std::mem::swap(&mut gen_params.octave_min, &mut gen_params.octave_max);
                    }

                    ui.scope(|ui| {
                        theme::field_style(ui);
                        const G2_LBL_W: f32 = 50.0;

                        ui.horizontal(|ui| {
                            field_label(ui, "OCT RANGE", G2_LBL_W);
                            // Words are proportional captions; the wells hold
                            // numbers only (contract, Type; round-20 #12).
                            field_label(ui, "MIN", 20.0);
                            let oct_max = gen_params.octave_max;
                            field_drag(
                                ui,
                                DragValue::new(&mut gen_params.octave_min)
                                    .range(-2..=oct_max)
                                    .custom_formatter(|v, _| fmt_octave(v)),
                                G_FIELD_W,
                            )
                            .on_hover_text("Lowest octave a generated note may use.");
                            field_label(ui, "MAX", 22.0);
                            let oct_min = gen_params.octave_min;
                            field_drag(
                                ui,
                                DragValue::new(&mut gen_params.octave_max)
                                    .range(oct_min..=2)
                                    .custom_formatter(|v, _| fmt_octave(v)),
                                G_FIELD_W,
                            )
                            .on_hover_text("Highest octave a generated note may use.");
                        });

                        ui.horizontal(|ui| {
                            field_label(ui, "OCT UP", G2_LBL_W);
                            field_drag(
                                ui,
                                DragValue::new(&mut gen_params.octave_up_prob)
                                    .speed(0.01)
                                    .range(0.0..=1.0)
                                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                                G_FIELD_W,
                            )
                            .on_hover_text(
                                "Chance that a note steps one octave up from the previous note.",
                            );
                        });

                        ui.horizontal(|ui| {
                            field_label(ui, "OCT DN", G2_LBL_W);
                            field_drag(
                                ui,
                                DragValue::new(&mut gen_params.octave_down_prob)
                                    .speed(0.01)
                                    .range(0.0..=1.0)
                                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                                G_FIELD_W,
                            )
                            .on_hover_text(
                                "Chance that a note steps one octave down from the previous note.",
                            );
                        });
                    });
                });

                ui.add(Separator::default().vertical().spacing(sep_w));

                // ── Group 3: PROBABILITIES ──────────────────────────────────
                ui.vertical(|ui| {
                    ui.set_width(col_w);
                    ui.spacing_mut().item_spacing = vec2(4.0, 3.0);

                    ui.add(GroupCaption::new("PROBABILITIES"));

                    ui.scope(|ui| {
                        theme::field_style(ui);
                        const G3_LBL_W: f32 = 40.0;

                        ui.horizontal(|ui| {
                            field_label(ui, "REST", G3_LBL_W);
                            field_drag(
                                ui,
                                DragValue::new(&mut gen_params.rest_probability)
                                    .speed(0.01)
                                    .range(0.0..=1.0)
                                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                                G_FIELD_W,
                            );

                            field_label(ui, "ACCENT", G3_LBL_W);
                            field_drag(
                                ui,
                                DragValue::new(&mut gen_params.accent_probability)
                                    .speed(0.01)
                                    .range(0.0..=1.0)
                                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                                G_FIELD_W,
                            );
                        });

                        ui.horizontal(|ui| {
                            field_label(ui, "SLIDE", G3_LBL_W);
                            field_drag(
                                ui,
                                DragValue::new(&mut gen_params.slide_probability)
                                    .speed(0.01)
                                    .range(0.0..=1.0)
                                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                                G_FIELD_W,
                            );

                            field_label(ui, "ACC BIAS", G3_LBL_W);
                            field_drag(
                                ui,
                                DragValue::new(&mut gen_params.accent_pitch_bias)
                                    .speed(0.01)
                                    .range(0.0..=1.0)
                                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                                G_FIELD_W,
                            )
                            .on_hover_text(
                                "Chance that an accented note moves to the upper half of the scale",
                            );
                        });

                        ui.add(ToggleButton::new(
                            &mut gen_params.slide_chaining,
                            "SLD CHAIN",
                        ))
                        .on_hover_text(
                            "Lit: slides may follow slides. Unlit: never two slides in a row.",
                        );
                    });
                });
            });
        },
    );
}

/// A full-width action tile of the GENERATION card, its label centred like
/// every other action's. egui aligns a Button's label by the layout it sits
/// in, and the card body is left-aligned (round-22 #12).
fn wide_action(ui: &mut Ui, label: &str) -> bool {
    let button = Button::new(
        nih_plug_egui::egui::RichText::new(label)
            .font(FontId::proportional(8.5))
            .color(theme::TEXT_PRIMARY),
    )
    .min_size(vec2(ui.available_width(), 22.0));
    ui.with_layout(Layout::top_down(Align::Center), |ui| {
        ui.add(button).clicked()
    })
    .inner
}

pub fn render_generation_panel(
    ui: &mut Ui,
    params: &Plugin303Params,
    gen_params: &mut GeneratorParams,
    setter: &ParamSetter,
) {
    DevicePanel::new("GENERATION", GENERATOR_H).show(
        ui,
        |_| {},
        |ui| {
            // Row 1: MUTATION, then SEED beside NEW SEED, the action that
            // changes it (round-22 #14). Only the field is a well: NEW SEED
            // stays a raised action.
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 3.0);
                ui.horizontal(|ui| {
                    field_label(ui, "MUTATION", 58.0);
                    ui.scope(|ui| {
                        theme::field_style(ui);
                        field_drag(
                            ui,
                            DragValue::new(&mut gen_params.mutation_rate)
                                .speed(0.01)
                                .range(0.0..=1.0)
                                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                            G_FIELD_W,
                        );
                    });

                    ui.add_space(16.0);

                    field_label(ui, "SEED", 36.0);
                    let seed_text = format!("{:04}", gen_params.seed % 10000);
                    ui.add(Readout::new(&seed_text, G_FIELD_W, G_ROW_H));
                    let new_seed_btn = Button::new(
                        nih_plug_egui::egui::RichText::new("NEW SEED")
                            .font(FontId::proportional(8.5))
                            .color(theme::TEXT_PRIMARY),
                    )
                    .min_size(vec2(0.0, G_ROW_H));
                    if ui.add(new_seed_btn).clicked() {
                        gen_params.seed = gen_params.seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
                    }
                });
            });

            ui.add_space(5.0);

            // Row 2: GENERATE, a raised action like MUTATE. Orange is for live
            // state, and nothing is live about an action at rest (round-20 #4).
            if wide_action(ui, "GENERATE") {
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

            ui.add_space(4.0);

            // Row 3: MUTATE, the same raised action tile.
            if wide_action(ui, "MUTATE") {
                if let Ok(mut seq_pat) = params.pattern.write() {
                    let mode = params.seq_mode.value();
                    let num_steps = params.seq_length.value() as u8;
                    let mut pat = seq_pat.compile(num_steps, mode);
                    // Independent Weyl sequence constant for mutation stream (distinct from NEW SEED)
                    gen_params.seed = gen_params.seed.wrapping_add(0x517C_C1B7_2722_0A95);
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
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn octave_formatter_signs_zero_like_the_knobs() {
        assert_eq!(fmt_octave(0.0), "+0");
        assert_eq!(fmt_octave(1.0), "+1");
        assert_eq!(fmt_octave(-1.0), "-1");
        assert_eq!(fmt_octave(0.5), "+0"); // truncates toward zero; range is integral
    }
}
