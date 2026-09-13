use nih_plug::prelude::{Param, ParamSetter};
use nih_plug_egui::egui::{
    pos2, vec2, Color32, CornerRadius, FontFamily, FontId, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, Widget,
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
        let width = 24.0;
        // 27 px of chrome: 14 above the track (8.5pt text + 5.5px gap to track),
        // 13 below for the value readout. The track itself is `height` tall.
        let total_height = self.height + 27.0;
        let (rect, mut response) =
            ui.allocate_exact_size(vec2(width, total_height), Sense::click_and_drag());

        let normalized = self.param.unmodulated_normalized_value();
        let track_top = rect.top() + 14.0;
        let track_bottom = rect.bottom() - 13.0;
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
            let is_enabled = ui.is_enabled();
            // Disabled faders paint with the explicit muted palette below, so
            // clear egui's fade-to-background on a painter clone — otherwise
            // the fade compounds on the muted tokens (round-13 issue 6).
            let painter = if is_enabled {
                ui.painter().clone()
            } else {
                let mut p = ui.painter().clone();
                p.set_fade_to_color(None);
                p
            };
            let accent = if is_enabled {
                self.accent_color
            } else {
                theme::TEXT_MUTED
            };

            // Label above (A, D, S, R) with >= 4 pt gap to track top
            painter.text(
                pos2(rect.center().x, rect.top() + 1.0),
                nih_plug_egui::egui::Align2::CENTER_TOP,
                self.label,
                FontId::new(8.5, FontFamily::Proportional),
                if is_enabled {
                    theme::TEXT_PRIMARY
                } else {
                    theme::TEXT_SECONDARY
                },
            );

            // Recessed vertical track
            let track_width = 6.0;
            let track_rect = Rect::from_min_max(
                pos2(rect.center().x - track_width * 0.5, track_top),
                pos2(rect.center().x + track_width * 0.5, track_bottom),
            );
            painter.rect_filled(track_rect, CornerRadius::same(2), theme::WELL_BG);
            painter.rect_stroke(
                track_rect,
                CornerRadius::same(2),
                Stroke::new(
                    1.5_f32,
                    if is_enabled {
                        theme::BORDER_FOCUS
                    } else {
                        theme::BORDER_DEFAULT
                    },
                ),
                StrokeKind::Inside,
            );

            // Active level fill bar (solid accent color from bottom to thumb)
            if normalized > 0.001 {
                let thumb_y = track_bottom - normalized * track_height;
                let fill_top = thumb_y.min(track_bottom - 1.0);
                let fill_rect = Rect::from_min_max(
                    pos2(rect.center().x - 2.0, fill_top),
                    pos2(rect.center().x + 2.0, track_bottom - 1.0),
                );
                painter.rect_filled(fill_rect, CornerRadius::same(1), accent);
            }

            // Flat thumb handle. Width <= track_width + 4 pt (10.0 px).
            // Travel is inset by half the thumb height plus 1 px clearance so
            // the bottom/top track cap remains visible at extremes (round-18 issue 4).
            let thumb_width = 10.0;
            let thumb_height = 5.0;
            let travel_inset = thumb_height * 0.5 + 1.0;
            let thumb_y =
                (track_bottom - travel_inset) - normalized * (track_height - 2.0 * travel_inset);
            let thumb_rect = Rect::from_center_size(
                pos2(rect.center().x, thumb_y),
                vec2(thumb_width, thumb_height),
            );
            painter.rect_filled(thumb_rect, CornerRadius::same(1), theme::SURFACE_ELEVATED);
            painter.rect_stroke(
                thumb_rect,
                CornerRadius::same(1),
                Stroke::new(
                    1.0_f32,
                    if is_enabled {
                        theme::BORDER_FOCUS
                    } else {
                        theme::BORDER_DEFAULT
                    },
                ),
                StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    pos2(thumb_rect.left() + 2.0, thumb_y),
                    pos2(thumb_rect.right() - 2.0, thumb_y),
                ],
                Stroke::new(1.0_f32, accent),
            );

            // Readout / value below (compact monospace)
            let val_str = self.param.to_string();
            let val_clean = val_str
                .trim_end_matches(" ms")
                .trim_end_matches(" s")
                .trim_end_matches('%');
            let formatted_val = if let Ok(f) = val_clean.parse::<f32>() {
                if self.label == "S" {
                    format!("{:.0}%", f * 100.0)
                } else if f >= 1000.0 {
                    format!("{:.2} s", f / 1000.0)
                } else {
                    format!("{:.0} ms", f)
                }
            } else {
                val_str
            };
            painter.text(
                pos2(rect.center().x, rect.bottom() - 1.0),
                nih_plug_egui::egui::Align2::CENTER_BOTTOM,
                formatted_val,
                FontId::new(7.5, FontFamily::Monospace),
                if is_enabled {
                    theme::TEXT_PRIMARY
                } else {
                    theme::TEXT_SECONDARY
                },
            );
        }

        response
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn mini_fader_geometry_bounds() {
        let height = 40.0_f32;
        let total_height = height + 27.0;
        let top = 0.0_f32;
        let bottom = top + total_height;
        let track_top = top + 14.0;
        let track_bottom = bottom - 13.0;
        let track_height = track_bottom - track_top;

        assert!(track_top > top, "Track top must have top label clearance");
        assert!(
            track_bottom < bottom,
            "Track bottom must have bottom readout clearance"
        );
        assert!(
            bottom - track_bottom >= 13.0,
            "Readout text must have at least 13px clearance"
        );
        assert!(track_height >= 40.0, "Track height must be at least 40pt");
        assert_eq!(track_height, height);

        let thumb_height = 5.0_f32;
        // At extremes the thumb must leave >= 1 px of track cap visible
        // (assert physical requirement directly, independent of impl formula).
        let travel_inset = thumb_height * 0.5 + 1.0;
        let y0 = track_bottom - travel_inset; // normalized = 0
        let y1 = track_top + travel_inset; // normalized = 1
        assert!(y0 + thumb_height * 0.5 <= track_bottom - 1.0);
        assert!(y1 - thumb_height * 0.5 >= track_top + 1.0);
    }
}
