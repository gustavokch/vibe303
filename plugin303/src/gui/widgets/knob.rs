use nih_plug::prelude::{Param, ParamSetter};
use nih_plug_egui::egui::{pos2, vec2, FontFamily, FontId, Response, Sense, Stroke, Ui, Widget};
use std::f32::consts::PI;

use crate::gui::theme;

pub struct Knob303<'a, P: Param> {
    param: &'a P,
    setter: &'a ParamSetter<'a>,
    label: &'a str,
    radius: f32,
}

impl<'a, P: Param> Knob303<'a, P> {
    pub fn new(param: &'a P, setter: &'a ParamSetter<'a>, label: &'a str, radius: f32) -> Self {
        Self {
            param,
            setter,
            label,
            radius,
        }
    }
}

impl<'a, P: Param> Widget for Knob303<'a, P> {
    fn ui(self, ui: &mut Ui) -> Response {
        let desired_size = vec2(self.radius * 2.0 + 20.0, self.radius * 2.0 + 38.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());

        let center = pos2(rect.center().x, rect.top() + self.radius + 6.0);
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
            let painter = ui.painter();

            // Tick arc (270 degrees: 135 deg to 405 deg)
            let start_angle = 135.0 * PI / 180.0;
            let end_angle = 405.0 * PI / 180.0;
            let current_angle = start_angle + normalized * (end_angle - start_angle);

            // Radial tick marks along 270-degree sweep
            let num_ticks = 11;
            for i in 0..num_ticks {
                let frac = i as f32 / (num_ticks - 1) as f32;
                let tick_angle = start_angle + frac * (end_angle - start_angle);
                let tick_in = self.radius + 3.0;
                let is_major = i == 0 || i == (num_ticks - 1) / 2 || i == num_ticks - 1;
                let tick_out = self.radius + if is_major { 7.5 } else { 5.5 };

                let p1 = pos2(
                    center.x + tick_angle.cos() * tick_in,
                    center.y + tick_angle.sin() * tick_in,
                );
                let p2 = pos2(
                    center.x + tick_angle.cos() * tick_out,
                    center.y + tick_angle.sin() * tick_out,
                );

                let stroke_color = if is_major {
                    theme::TEXT_SECONDARY
                } else {
                    theme::PANEL_BORDER_HIGHLIGHT
                };
                let stroke_width = if is_major { 1.4_f32 } else { 1.0_f32 };
                painter.line_segment([p1, p2], Stroke::new(stroke_width, stroke_color));
            }

            // Outer bezel ring
            painter.circle_filled(center, self.radius + 2.0, theme::KNOB_BEZEL);
            painter.circle_stroke(
                center,
                self.radius + 2.0,
                Stroke::new(1.0_f32, theme::PANEL_BORDER),
            );

            // Metallic gradient disc
            painter.circle_filled(center, self.radius, theme::KNOB_BODY_DARK);
            painter.circle_filled(
                pos2(center.x - self.radius * 0.2, center.y - self.radius * 0.2),
                self.radius * 0.8,
                theme::KNOB_BODY_LIGHT,
            );

            // Pointer line
            let pointer_len = self.radius * 0.85;
            let pointer_end = pos2(
                center.x + current_angle.cos() * pointer_len,
                center.y + current_angle.sin() * pointer_len,
            );
            let pointer_start = pos2(
                center.x + current_angle.cos() * (self.radius * 0.15),
                center.y + current_angle.sin() * (self.radius * 0.15),
            );
            painter.line_segment(
                [pointer_start, pointer_end],
                Stroke::new(2.5_f32, theme::KNOB_POINTER),
            );

            // Parameter Title Label below dial
            let label_y = center.y + self.radius + 10.0;
            let label_pos = pos2(rect.center().x, label_y);
            painter.text(
                pos2(label_pos.x + 0.3, label_pos.y),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                self.label.to_uppercase(),
                FontId::new(9.0, FontFamily::Proportional),
                theme::TEXT_PRIMARY,
            );
            painter.text(
                label_pos,
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                self.label.to_uppercase(),
                FontId::new(9.0, FontFamily::Proportional),
                theme::TEXT_PRIMARY,
            );

            // Human-readable formatted value readout below title (with physical units)
            let raw_str = self.param.to_string();
            let value_str = if let Ok(f) = raw_str.parse::<f32>() {
                if self.label.contains("CUTOFF") {
                    let hz = 300.0 * (15000.0 / 300.0_f32).powf(f.clamp(0.0, 1.0));
                    if hz >= 1000.0 {
                        format!("{:.1} kHz", hz / 1000.0)
                    } else {
                        format!("{:.0} Hz", hz)
                    }
                } else if self.label.contains("RESONANCE")
                    || self.label.contains("ENV MOD")
                    || self.label.contains("ACCENT")
                    || self.label.contains("VOLUME")
                    || self.label.contains("MIX")
                    || self.label.contains("DRIVE")
                    || self.label.contains("RESO")
                    || self.label.contains("PW")
                    || self.label.contains("VOL")
                    || self.label.contains("DRIFT")
                    || self.label.contains("AGE")
                {
                    if f <= 1.0 && f >= 0.0 {
                        format!("{:.0}%", f * 100.0)
                    } else if f.abs() >= 100.0 {
                        format!("{:.0}%", f)
                    } else {
                        format!("{:.1}%", f)
                    }
                } else if self.label.contains("DECAY") {
                    let ms = 200.0 + f.clamp(0.0, 1.0) * 1800.0;
                    if ms >= 1000.0 {
                        format!("{:.2} s", ms / 1000.0)
                    } else {
                        format!("{:.0} ms", ms)
                    }
                } else if self.label.contains("GLIDE") {
                    let ms = f.clamp(0.0, 1.0) * 500.0;
                    format!("{:.0} ms", ms)
                } else if self.label.contains("TUNING") {
                    format!("{:+.0} ct", f)
                } else if self.label.contains("DET") {
                    format!("{:+.1} st", f)
                } else if self.label.contains("TEMPO") {
                    format!("{:.0} BPM", f)
                } else if f.abs() >= 100.0 {
                    format!("{:.0}", f)
                } else if f.abs() >= 10.0 {
                    format!("{:.1}", f)
                } else {
                    format!("{:.2}", f)
                }
            } else {
                raw_str
            };
            painter.text(
                pos2(rect.center().x, rect.bottom() - 4.0),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                value_str,
                FontId::new(8.0, FontFamily::Monospace),
                theme::TEXT_MUTED,
            );
        }

        response
    }
}
