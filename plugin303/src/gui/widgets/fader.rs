use nih_plug::prelude::{Param, ParamSetter};
use nih_plug_egui::egui::{
    pos2, vec2, Color32, FontFamily, FontId, Rect, Response, Sense, Ui, Widget,
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
    /// Widget height for a fader whose track is `track_height` tall.
    pub fn footprint_height(track_height: f32) -> f32 {
        track_height + 23.0
    }

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
        // CHROME_H px around the track: 12 above (8.5 pt label), 11 below (value).
        let total_height = Self::footprint_height(self.height);
        let (rect, mut response) =
            ui.allocate_exact_size(vec2(width, total_height), Sense::click_and_drag());

        let normalized = self.param.unmodulated_normalized_value();
        let track_top = rect.top() + 12.0;
        let track_bottom = rect.bottom() - 11.0;
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
                    theme::TEXT_SECONDARY
                } else {
                    theme::TEXT_MUTED
                },
            );

            // Flat bar fader: an inset well filled from the bottom up to the
            // value, like a Live device slider. No thumb, no outline.
            let track_width = 12.0;
            let track_rect = Rect::from_min_max(
                pos2(rect.center().x - track_width * 0.5, track_top),
                pos2(rect.center().x + track_width * 0.5, track_bottom),
            );
            let track_fill = if is_enabled && response.hovered() {
                theme::SURFACE_PRESSED
            } else {
                theme::WELL_BG
            };
            painter.rect_filled(track_rect, theme::radius(), track_fill);
            let fill_top = track_bottom - normalized * track_height;
            if track_bottom - fill_top >= 1.0 {
                let fill_rect = Rect::from_min_max(
                    pos2(track_rect.left(), fill_top),
                    track_rect.right_bottom(),
                );
                painter.rect_filled(fill_rect, theme::radius(), accent);
            }

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
