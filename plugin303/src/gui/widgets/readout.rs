use nih_plug_egui::egui::{vec2, Align2, FontId, Response, Sense, Ui, Vec2, Widget};

use crate::gui::theme;

/// A value well that cannot be edited: a value in a disabled group, or a
/// read-only figure such as the generator seed. egui's disabled fade would dim
/// a built-in DragValue or ComboBox on top of its token, so this paints the
/// tokens itself: a `WELL_BG` well with `TEXT_SECONDARY` monospace text.
pub struct Readout<'a> {
    text: &'a str,
    size: Vec2,
}

impl<'a> Readout<'a> {
    pub fn new(text: &'a str, width: f32, height: f32) -> Self {
        Self {
            text,
            size: vec2(width, height),
        }
    }
}

impl Widget for Readout<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(self.size, Sense::hover());
        if ui.is_rect_visible(rect) {
            let mut painter = ui.painter().clone();
            painter.set_fade_to_color(None);
            painter.rect_filled(rect, theme::radius(), theme::WELL_BG);
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                self.text,
                FontId::monospace(9.0),
                theme::TEXT_SECONDARY,
            );
        }
        response
    }
}
