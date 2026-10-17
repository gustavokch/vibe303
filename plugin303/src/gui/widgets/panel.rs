use nih_plug_egui::egui::{
    pos2, vec2, Align, Align2, CornerRadius, FontFamily, FontId, Layout, Rect, Sense, Ui, UiBuilder,
};

use crate::gui::theme;

/// Height of the title strip at the top of every device.
pub const HEADER_H: f32 = 22.0;
/// Inset of header and body content from the device edge.
pub const PAD_X: f32 = 8.0;
pub const PAD_Y: f32 = 4.0;

/// A flat device card: a darker body plane under a lighter title strip, no
/// outline. Like a Bitwig device, the header carries the name on the left and
/// that device's mode switches on the right; the body holds the controls.
pub struct DevicePanel<'a> {
    title: &'a str,
    height: f32,
    dimmed: bool,
}

impl<'a> DevicePanel<'a> {
    /// `height` is the whole card, header included. The card is as wide as the
    /// Ui it is shown in.
    pub fn new(title: &'a str, height: f32) -> Self {
        Self {
            title,
            height,
            dimmed: false,
        }
    }

    /// Mute the title: the device is bypassed or inactive.
    pub fn dimmed(mut self, dimmed: bool) -> Self {
        self.dimmed = dimmed;
        self
    }

    /// Height left for body content inside a card of `height`.
    pub fn body_height(height: f32) -> f32 {
        height - HEADER_H - 2.0 * PAD_Y
    }

    /// Draw the card. `header` fills the strip right-to-left (first item added
    /// sits flush right); `body` fills the area below, top-down from the
    /// top-left. The card's full rect is allocated up front, so neither
    /// closure can move the parent's cursor.
    pub fn show<R>(
        self,
        ui: &mut Ui,
        header: impl FnOnce(&mut Ui),
        body: impl FnOnce(&mut Ui) -> R,
    ) -> R {
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(vec2(width, self.height), Sense::hover());
        let header_rect = Rect::from_min_size(rect.min, vec2(width, HEADER_H));
        let body_rect = Rect::from_min_max(pos2(rect.left(), header_rect.bottom()), rect.max);

        if ui.is_rect_visible(rect) {
            let p = ui.painter();
            p.rect_filled(rect, theme::radius(), theme::PANEL_BG);
            p.rect_filled(
                header_rect,
                CornerRadius {
                    nw: theme::RADIUS,
                    ne: theme::RADIUS,
                    sw: 0,
                    se: 0,
                },
                theme::PANEL_HEADER_BG,
            );
            p.text(
                pos2(header_rect.left() + PAD_X, header_rect.center().y),
                Align2::LEFT_CENTER,
                self.title,
                FontId::new(9.0, FontFamily::Proportional),
                if self.dimmed {
                    theme::TEXT_MUTED
                } else {
                    theme::TEXT_PRIMARY
                },
            );
        }

        let id = ui.id().with(("device_panel", self.title));
        let mut header_ui = ui.new_child(
            UiBuilder::new()
                .id_salt(id.with("header"))
                .max_rect(header_rect.shrink2(vec2(PAD_X, 0.0)))
                .layout(Layout::right_to_left(Align::Center)),
        );
        header(&mut header_ui);

        let mut body_ui = ui.new_child(
            UiBuilder::new()
                .id_salt(id.with("body"))
                .max_rect(body_rect.shrink2(vec2(PAD_X, PAD_Y)))
                .layout(Layout::top_down(Align::Min)),
        );
        body(&mut body_ui)
    }
}
