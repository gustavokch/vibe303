use nih_plug::prelude::{Param, ParamSetter};
use nih_plug_egui::egui::{
    pos2, vec2, FontFamily, FontId, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Widget,
};
use std::f32::consts::PI;

use crate::gui::theme;

pub struct Knob303<'a, P: Param> {
    param: &'a P,
    setter: &'a ParamSetter<'a>,
    label: &'a str,
    radius: f32,
    accent_color: Option<nih_plug_egui::egui::Color32>,
    value_text: Option<String>,
}

impl<'a, P: Param> Knob303<'a, P> {
    pub fn new(param: &'a P, setter: &'a ParamSetter<'a>, label: &'a str, radius: f32) -> Self {
        Self {
            param,
            setter,
            label,
            radius,
            accent_color: None,
            value_text: None,
        }
    }

    /// Footprint of a knob with outer radius `radius`: the dial, then the label
    /// row and the value well stacked tight beneath it.
    pub fn size(radius: f32) -> nih_plug_egui::egui::Vec2 {
        vec2(radius * 2.0 + 16.0, radius * 2.0 + 26.0)
    }

    pub fn with_accent(mut self, color: nih_plug_egui::egui::Color32) -> Self {
        self.accent_color = Some(color);
        self
    }

    /// Readout text when the parameter's own text cannot know it: a value whose
    /// meaning depends on another parameter (CUTOFF and GLIDE follow ADV ENV).
    pub fn value_text(mut self, text: String) -> Self {
        self.value_text = Some(text);
        self
    }
}

impl<'a, P: Param<Plain = f32>> Widget for Knob303<'a, P> {
    fn ui(self, ui: &mut Ui) -> Response {
        let desired_size = Self::size(self.radius);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());

        let center = pos2(rect.center().x, rect.top() + self.radius + 2.0);
        let normalized = self.param.unmodulated_normalized_value();

        // Interaction
        if response.drag_started() {
            self.setter.begin_set_parameter(self.param);
        }
        if response.dragged() {
            let drag_delta = -response.drag_delta().y;
            let sensitivity = if ui.input(|i| i.modifiers.shift) {
                0.001
            } else {
                0.005
            };
            let new_val = (normalized + drag_delta * sensitivity).clamp(0.0, 1.0);
            self.setter.set_parameter_normalized(self.param, new_val);
            response.mark_changed();
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
            // Disabled knobs already pick muted tokens below. egui's
            // fade-to-background would dim them a second time and drop the
            // needle and value arc to 2.3:1, so clear the fade on a painter
            // clone and let the tokens be the only dimming (round-14 issue 3,
            // same treatment as MiniFader).
            let painter = if is_enabled {
                ui.painter().clone()
            } else {
                let mut p = ui.painter().clone();
                p.set_fade_to_color(None);
                p
            };
            let is_hovered = response.hovered();

            // 1. Flat dial, Ableton style: a thin value ring around a flat
            // body disc with a pointer line. No outline, no ticks, no bezel.
            let start_angle = 135.0 * PI / 180.0;
            let end_angle = 405.0 * PI / 180.0;
            let arc_w = (self.radius * 0.14).clamp(2.0, 3.0);
            let ring_r = self.radius - arc_w * 0.5;
            let body_r = self.radius * 0.68;

            let arc_points = |from: f32, to: f32| -> Vec<Pos2> {
                const ARC_SEGMENTS: usize = 36;
                let steps = (((to - from).abs() * ARC_SEGMENTS as f32).ceil() as usize).max(2);
                (0..=steps)
                    .map(|i| {
                        let n = from + (to - from) * i as f32 / steps as f32;
                        let a = start_angle + n * (end_angle - start_angle);
                        pos2(center.x + a.cos() * ring_r, center.y + a.sin() * ring_r)
                    })
                    .collect()
            };

            // 2. Value ring: full-sweep track, then the active fill. Polarity
            // comes from the parameter's range: a bipolar range (plain minimum
            // < 0, e.g. TUNING / PITCH / dB trims) fills outward from the
            // normalized position of 0.0; a unipolar range fills from the
            // minimum (round-13 issue 1).
            painter.add(Shape::line(
                arc_points(0.0, 1.0),
                Stroke::new(arc_w, theme::KNOB_TRACK),
            ));
            let accent = if is_enabled {
                self.accent_color.unwrap_or(theme::ACCENT_ORANGE)
            } else {
                theme::TEXT_MUTED
            };
            let zero_norm = self.param.preview_normalized(0.0);
            let is_bipolar = self.param.preview_plain(0.0) < 0.0;
            let (fill_from, fill_to) = if is_bipolar {
                (zero_norm.min(normalized), zero_norm.max(normalized))
            } else {
                (0.0_f32, normalized)
            };
            if fill_to - fill_from > 0.001 {
                painter.add(Shape::line(
                    arc_points(fill_from, fill_to),
                    Stroke::new(arc_w, accent),
                ));
            }

            // 3. Body disc and pointer.
            let body_fill = if is_hovered && is_enabled {
                theme::KNOB_BODY_HOVER
            } else {
                theme::KNOB_BODY
            };
            painter.circle_filled(center, body_r, body_fill);

            let current_angle = compute_needle_angle(normalized, start_angle, end_angle);
            let pointer_color = if is_enabled {
                theme::TEXT_PRIMARY
            } else {
                theme::TEXT_MUTED
            };
            painter.line_segment(
                [
                    pos2(
                        center.x + current_angle.cos() * body_r * 0.35,
                        center.y + current_angle.sin() * body_r * 0.35,
                    ),
                    pos2(
                        center.x + current_angle.cos() * (body_r - 1.0),
                        center.y + current_angle.sin() * (body_r - 1.0),
                    ),
                ],
                Stroke::new(
                    if self.radius >= 18.0 {
                        2.0_f32
                    } else {
                        1.5_f32
                    },
                    pointer_color,
                ),
            );

            // 4. Parameter Label
            let label_y = center.y + self.radius + 7.0;
            let label_color = if is_enabled {
                theme::TEXT_SECONDARY
            } else {
                theme::TEXT_MUTED
            };
            painter.text(
                pos2(rect.center().x, label_y),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                self.label.to_uppercase(),
                FontId::new(8.5, FontFamily::Proportional),
                label_color,
            );

            // 5. Monospace value badge: the parameter's own text, the same
            // string the host shows.
            let value_y = label_y + 11.0;
            let value_str = match &self.value_text {
                Some(text) => text.clone(),
                None => self.param.to_string(),
            };

            let pill_width = if self.radius >= theme::KNOB_RADIUS_LARGE - 0.5 {
                theme::KNOB_PILL_WIDTH_LARGE
            } else if self.radius >= theme::KNOB_RADIUS_MEDIUM - 0.5 {
                theme::KNOB_PILL_WIDTH_MEDIUM
            } else {
                theme::KNOB_PILL_WIDTH_SMALL
            };
            let pill_rect =
                Rect::from_center_size(pos2(rect.center().x, value_y), vec2(pill_width, 11.0));
            painter.rect_filled(pill_rect, theme::radius(), theme::WELL_BG);
            painter.text(
                pos2(rect.center().x, value_y),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                value_str,
                FontId::new(8.0, FontFamily::Monospace),
                theme::TEXT_PRIMARY,
            );
        }

        response
    }
}

/// Compute the pointer needle angle for a knob from normalized position (0.0..=1.0)
/// by linearly interpolating between `start_angle` and `end_angle`.
pub fn compute_needle_angle(normalized: f32, start_angle: f32, end_angle: f32) -> f32 {
    start_angle + normalized * (end_angle - start_angle)
}

#[cfg(test)]
mod needle_tests {
    use super::compute_needle_angle;
    use std::f32::consts::PI;

    #[test]
    fn needle_angle_positions() {
        let start = 0.75 * PI;
        let end = 2.25 * PI;

        // Exact endpoints
        assert_eq!(compute_needle_angle(0.0, start, end), start);
        assert!((compute_needle_angle(1.0, start, end) - end).abs() < 1e-5);

        // Center / 12 o'clock for bipolar (normalized = 0.5)
        let mid = compute_needle_angle(0.5, start, end);
        assert!((mid - 1.5 * PI).abs() < 1e-5);

        // Monotonic lerp across quarter steps
        let q1 = compute_needle_angle(0.25, start, end);
        let q3 = compute_needle_angle(0.75, start, end);
        assert!(start < q1 && q1 < mid && mid < q3 && q3 < end);
        assert!((q1 - (start + 0.25 * (end - start))).abs() < 1e-5);
        assert!((q3 - (start + 0.75 * (end - start))).abs() < 1e-5);
    }
}
