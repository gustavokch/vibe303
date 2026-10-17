use nih_plug_egui::egui::{vec2, Align2, Color32, FontFamily, FontId, Response, Sense, Ui, Widget};

use crate::gui::theme;

/// Default height of a toggle and flat button: one height, one row rhythm.
pub const TOGGLE_H: f32 = 18.0;

/// A flat on/off tile. Lit orange when on, a quiet grey tile when off. Replaces
/// egui's checkbox so every boolean in the UI looks and reads the same.
pub struct ToggleButton<'a> {
    on: &'a mut bool,
    label: &'a str,
    min_width: f32,
    height: f32,
}

impl<'a> ToggleButton<'a> {
    pub fn new(on: &'a mut bool, label: &'a str) -> Self {
        Self {
            on,
            label,
            min_width: 0.0,
            height: TOGGLE_H,
        }
    }

    /// Floor for the tile width, so a group of toggles can share one width.
    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    /// Tile height, for rows that run taller than the default.
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }
}

/// Fill and text for a toggle in each state.
fn toggle_colors(on: bool, hovered: bool, enabled: bool) -> (Color32, Color32) {
    match (on, enabled) {
        (true, true) if hovered => (theme::ACCENT_ORANGE_HOVER, theme::TEXT_ON_ACCENT),
        (true, true) => (theme::ACCENT_ORANGE, theme::TEXT_ON_ACCENT),
        (true, false) => (theme::SURFACE_HOVER, theme::TEXT_SECONDARY),
        (false, true) if hovered => (theme::SURFACE_HOVER, theme::TEXT_PRIMARY),
        (false, true) => (theme::SURFACE_ELEVATED, theme::TEXT_SECONDARY),
        (false, false) => (theme::SURFACE_PRESSED, theme::TEXT_MUTED),
    }
}

impl Widget for ToggleButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let font = FontId::new(8.5, FontFamily::Proportional);
        let text_w = ui.fonts(|f| {
            f.layout_no_wrap(self.label.to_string(), font.clone(), Color32::WHITE)
                .size()
                .x
        });
        let width = (text_w + 16.0).max(self.min_width);
        let (rect, mut response) = ui.allocate_exact_size(vec2(width, self.height), Sense::click());
        if response.clicked() {
            *self.on = !*self.on;
            response.mark_changed();
        }
        if ui.is_rect_visible(rect) {
            let enabled = ui.is_enabled();
            let mut painter = ui.painter().clone();
            painter.set_fade_to_color(None);
            let (fill, text) = toggle_colors(*self.on, response.hovered(), enabled);
            painter.rect_filled(rect, theme::radius(), fill);
            painter.text(rect.center(), Align2::CENTER_CENTER, self.label, font, text);
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A disabled toggle must never light up in the accent colour, even when
    /// it is on: lit orange means "live", and a disabled control is not.
    #[test]
    fn disabled_toggle_never_claims_accent() {
        for on in [true, false] {
            for hovered in [true, false] {
                let (fill, _) = toggle_colors(on, hovered, false);
                assert_ne!(fill, theme::ACCENT_ORANGE);
                assert_ne!(fill, theme::ACCENT_ORANGE_HOVER);
            }
        }
    }
}
