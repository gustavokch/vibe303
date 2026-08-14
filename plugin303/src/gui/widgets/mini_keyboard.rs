use nih_plug_egui::egui::{vec2, Color32, FontFamily, FontId, Ui};
use crate::gui::theme;

pub struct MiniKeyboard<'a> {
    semitone: &'a mut i8,
}

impl<'a> MiniKeyboard<'a> {
    pub fn new(semitone: &'a mut i8) -> Self {
        Self { semitone }
    }

    pub fn show(self, ui: &mut Ui) -> bool {
        let mut changed = false;
        let note_names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B", "C'"];
        let is_black = [false, true, false, true, false, false, true, false, true, false, true, false, false];

        ui.horizontal(|ui| {
            for (idx, &name) in note_names.iter().enumerate() {
                let selected = *self.semitone == idx as i8;
                let bg_color = if selected {
                    theme::ACCENT_AMBER
                } else if is_black[idx] {
                    Color32::from_rgb(0x20, 0x24, 0x2c)
                } else {
                    Color32::from_rgb(0xe0, 0xe4, 0xec)
                };

                let text_color = if selected {
                    Color32::BLACK
                } else if is_black[idx] {
                    Color32::WHITE
                } else {
                    Color32::BLACK
                };

                let btn = nih_plug_egui::egui::Button::new(
                    nih_plug_egui::egui::RichText::new(name)
                        .font(FontId::new(9.0, FontFamily::Monospace))
                        .color(text_color),
                )
                .fill(bg_color)
                .min_size(vec2(16.0, 28.0));

                if ui.add(btn).clicked() {
                    *self.semitone = idx as i8;
                    changed = true;
                }
            }
        });

        changed
    }
}
