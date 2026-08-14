use nih_plug_egui::egui::{
    vec2, Color32, FontFamily, FontId, Sense, Ui,
};
use crate::gui::theme;
use crate::params::{SequencerStep, StepGate};

pub fn format_pitch(semitone: i8, octave: i8) -> String {
    let note_names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B", "C"];
    let name = note_names.get(semitone.clamp(0, 12) as usize).unwrap_or(&"C");
    let oct_num = 3 + octave;
    format!("{}{}", name, oct_num)
}

pub fn render_step_strip(
    ui: &mut Ui,
    step_idx: usize,
    step: &mut SequencerStep,
    is_active: bool,
) -> bool {
    let mut changed = false;
    let width = 54.0;

    ui.vertical(|ui| {
        ui.set_width(width);

        // 1. Playhead LED bar
        let (led_rect, _) = ui.allocate_exact_size(vec2(width, 6.0), Sense::hover());
        if is_active {
            ui.painter().rect_filled(led_rect, 2.0, theme::PLAYHEAD_RED);
            ui.painter().rect_filled(led_rect.expand(2.0), 3.0, theme::PLAYHEAD_RED_GLOW);
        } else {
            ui.painter().rect_filled(led_rect, 2.0, Color32::from_rgb(0x22, 0x26, 0x2e));
        }

        ui.add_space(2.0);

        // Step number label
        ui.label(
            nih_plug_egui::egui::RichText::new(format!("{:02}", step_idx + 1))
                .font(FontId::new(9.5, FontFamily::Monospace))
                .color(if is_active { theme::TEXT_PRIMARY } else { theme::TEXT_MUTED }),
        );

        // 2. Modifiers: ACC and SLD
        ui.horizontal(|ui| {
            let acc_btn = nih_plug_egui::egui::Button::new("ACC")
                .fill(if step.accent { theme::ACCENT_AMBER } else { Color32::from_rgb(0x24, 0x28, 0x30) })
                .min_size(vec2(24.0, 16.0));
            if ui.add(acc_btn).clicked() {
                step.accent = !step.accent;
                changed = true;
            }

            let sld_btn = nih_plug_egui::egui::Button::new("SLD")
                .fill(if step.slide { theme::SLIDE_CYAN } else { Color32::from_rgb(0x24, 0x28, 0x30) })
                .min_size(vec2(24.0, 16.0));
            if ui.add(sld_btn).clicked() {
                step.slide = !step.slide;
                changed = true;
            }
        });

        ui.add_space(2.0);

        // 3. Gate Badge: Note / Tie / Rest
        let (gate_str, gate_color) = match step.gate {
            StepGate::Note => ("NOTE", theme::GATE_NOTE_COLOR),
            StepGate::Tie => ("TIE", theme::GATE_TIE_COLOR),
            StepGate::Rest => ("REST", theme::GATE_REST_COLOR),
        };
        let gate_btn = nih_plug_egui::egui::Button::new(
            nih_plug_egui::egui::RichText::new(gate_str)
                .font(FontId::new(9.0, FontFamily::Proportional))
                .color(if step.gate == StepGate::Rest { theme::TEXT_MUTED } else { Color32::BLACK }),
        )
        .fill(gate_color)
        .min_size(vec2(width, 22.0));

        if ui.add(gate_btn).clicked() {
            step.gate = match step.gate {
                StepGate::Note => StepGate::Tie,
                StepGate::Tie => StepGate::Rest,
                StepGate::Rest => StepGate::Note,
            };
            changed = true;
        }

        ui.add_space(2.0);

        // 4. Pitch & Octave
        let pitch_str = format_pitch(step.semitone, step.octave);
        ui.label(
            nih_plug_egui::egui::RichText::new(pitch_str)
                .font(FontId::new(11.0, FontFamily::Monospace))
                .color(theme::TEXT_PRIMARY),
        );

        ui.horizontal(|ui| {
            if ui.small_button("▲").clicked() {
                if step.octave < 2 {
                    step.octave += 1;
                    changed = true;
                }
            }
            if ui.small_button("▼").clicked() {
                if step.octave > -2 {
                    step.octave -= 1;
                    changed = true;
                }
            }
        });
    });

    changed
}
