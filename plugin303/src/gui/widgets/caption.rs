use nih_plug_egui::egui::{pos2, vec2, Align2, FontFamily, FontId, Response, Sense, Ui, Widget};

use crate::gui::theme;

/// Height of a group caption row.
pub const GROUP_CAPTION_H: f32 = 11.0;
/// Gap between the caption text and its rule.
const RULE_GAP: f32 = 6.0;

/// The caption of a group of fields inside a device body: 8.5 pt words, then
/// the separator line out to the group's right edge. The field labels under a
/// caption share its size and colour; the rule is what makes it a caption
/// (round-22 #21). In a disabled group the text turns `TEXT_MUTED`, painted
/// with egui's fade cleared like every custom widget.
pub struct GroupCaption<'a> {
    text: &'a str,
}

impl<'a> GroupCaption<'a> {
    pub fn new(text: &'a str) -> Self {
        Self { text }
    }
}

impl Widget for GroupCaption<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), GROUP_CAPTION_H), Sense::hover());
        if ui.is_rect_visible(rect) {
            let mut painter = ui.painter().clone();
            painter.set_fade_to_color(None);
            let color = if ui.is_enabled() {
                theme::TEXT_SECONDARY
            } else {
                theme::TEXT_MUTED
            };
            let text = painter.text(
                pos2(rect.left(), rect.center().y),
                Align2::LEFT_CENTER,
                self.text,
                FontId::new(8.5, FontFamily::Proportional),
                color,
            );
            let rule_x = text.right() + RULE_GAP;
            if rule_x < rect.right() {
                // The separator's own stroke: the only line in the UI.
                painter.hline(
                    rule_x..=rect.right(),
                    painter.round_to_pixel_center(rect.center().y),
                    ui.visuals().widgets.noninteractive.bg_stroke,
                );
            }
        }
        response
    }
}
