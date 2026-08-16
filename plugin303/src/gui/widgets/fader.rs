use nih_plug::prelude::{Param, ParamSetter};
use nih_plug_egui::egui::{
    pos2, vec2, Color32, FontFamily, FontId, Rect, Response, Sense, Stroke, StrokeKind, Ui, Widget,
};

use crate::gui::theme;

pub struct MiniFader<'a, P: Param> {
    param: &'a P,
    setter: &'a ParamSetter<'a>,
    label: &'a str,
    height: f32,
    accent_color: Color32,
}

impl<'a, P: Param> MiniFader<'a, P> {
    pub fn new(
        param: &'a P,
        setter: &'a ParamSetter<'a>,
        label: &'a str,
        height: f32,
        accent_color: Color32,
    ) -> Self {
        Self {
            param,
            setter,
            label,
            height,
            accent_color,
        }
    }
}

impl<'a, P: Param> Widget for MiniFader<'a, P> {
    fn ui(self, ui: &mut Ui) -> Response {
        let width = 28.0;
        let total_height = self.height + 30.0;
        let (rect, mut response) =
            ui.allocate_exact_size(vec2(width, total_height), Sense::click_and_drag());

        let normalized = self.param.unmodulated_normalized_value();
        let track_top = rect.top() + 14.0;
        let track_bottom = rect.bottom() - 14.0;
        let track_height = track_bottom - track_top;

        // Interaction
        if response.drag_started() {
            self.setter.begin_set_parameter(self.param);
        }
        if response.dragged() || response.clicked() {
            if let Some(mouse_pos) = response.interact_pointer_pos() {
                let frac = 1.0 - ((mouse_pos.y - track_top) / track_height).clamp(0.0, 1.0);
                self.setter.set_parameter_normalized(self.param, frac);
                response.mark_changed();
            }
        }
        if response.drag_stopped() {
            self.setter.end_set_parameter(self.param);
        }
        if response.double_clicked() {
            self.setter.begin_set_parameter(self.param);
            self.setter
                .set_parameter_normalized(self.param, self.param.default_normalized_value());
            self.setter.end_set_parameter(self.param);
            response.mark_changed();
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();

            // Label above (A, D, S, R)
            painter.text(
                pos2(rect.center().x, rect.top() + 2.0),
                nih_plug_egui::egui::Align2::CENTER_TOP,
                self.label,
                FontId::new(9.5, FontFamily::Proportional),
                theme::TEXT_PRIMARY,
            );

            // Recessed vertical track
            let track_width = 6.0;
            let track_rect = Rect::from_min_max(
                pos2(rect.center().x - track_width * 0.5, track_top),
                pos2(rect.center().x + track_width * 0.5, track_bottom),
            );
            painter.rect_filled(track_rect, 3.0, Color32::from_rgb(0x10, 0x12, 0x16));
            painter.rect_stroke(
                track_rect,
                3.0,
                Stroke::new(1.0_f32, theme::PANEL_BORDER),
                StrokeKind::Inside,
            );

            // Active level fill bar (vivid glowing orange accent color from bottom to thumb)
            let thumb_y = track_bottom - normalized * track_height;
            let mut fill_top = thumb_y.min(track_bottom - 1.0);
            if fill_top > track_bottom - 3.0 {
                fill_top = track_bottom - 3.0;
            }
            let fill_rect = Rect::from_min_max(
                pos2(rect.center().x - 2.0, fill_top),
                pos2(rect.center().x + 2.0, track_bottom),
            );
            // Outer bright orange bloom glow
            let glow_color = Color32::from_rgba_premultiplied(
                self.accent_color.r(),
                self.accent_color.g(),
                self.accent_color.b(),
                0x50,
            );
            painter.rect_filled(fill_rect.expand(1.0), 1.0, glow_color);
            // Vivid solid orange core
            painter.rect_filled(fill_rect, 1.0, self.accent_color);
            // Vivid solid orange core
            painter.rect_filled(fill_rect, 1.0, self.accent_color);

            // Fader handle / thumb
            let thumb_width = 16.0;
            let thumb_height = 6.0;
            let thumb_rect = Rect::from_center_size(
                pos2(rect.center().x, thumb_y),
                vec2(thumb_width, thumb_height),
            );
            painter.rect_filled(thumb_rect, 1.5, theme::KNOB_BODY_LIGHT);
            painter.rect_stroke(
                thumb_rect,
                1.5,
                Stroke::new(1.0_f32, theme::KNOB_BEZEL),
                StrokeKind::Inside,
            );
            // Center indicator notch on thumb
            painter.line_segment(
                [
                    pos2(thumb_rect.left() + 2.0, thumb_y),
                    pos2(thumb_rect.right() - 2.0, thumb_y),
                ],
                Stroke::new(1.0_f32, Color32::BLACK),
            );

            // Readout / value below (compact formatting to prevent horizontal collisions)
            let val_str = self.param.to_string();
            let num_str: String = val_str.chars().filter(|c| c.is_digit(10) || *c == '.' || *c == '-').collect();
            let formatted_val = if let Ok(f) = num_str.parse::<f32>() {
                if self.label == "S" {
                    format!("{:.0}%", f * 100.0)
                } else if f >= 1000.0 {
                    format!("{:.1}s", f / 1000.0)
                } else {
                    format!("{:.0}ms", f)
                }
            } else {
                val_str.replace(" ms", "ms").replace(" s", "s")
            };
            painter.text(
                pos2(rect.center().x, rect.bottom() - 1.0),
                nih_plug_egui::egui::Align2::CENTER_BOTTOM,
                formatted_val,
                FontId::new(7.5, FontFamily::Monospace),
                theme::TEXT_MUTED,
            );
        }

        response
    }
}
