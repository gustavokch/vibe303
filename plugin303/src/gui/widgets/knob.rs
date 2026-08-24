use nih_plug::prelude::{Param, ParamSetter};
use nih_plug_egui::egui::{
    pos2, vec2, CornerRadius, FontFamily, FontId, Rect, Response, Sense, Stroke, Ui, Widget,
};
use std::f32::consts::PI;

use crate::gui::theme;

pub struct Knob303<'a, P: Param> {
    param: &'a P,
    setter: &'a ParamSetter<'a>,
    label: &'a str,
    radius: f32,
    accent_color: Option<nih_plug_egui::egui::Color32>,
}

impl<'a, P: Param> Knob303<'a, P> {
    pub fn new(param: &'a P, setter: &'a ParamSetter<'a>, label: &'a str, radius: f32) -> Self {
        Self {
            param,
            setter,
            label,
            radius,
            accent_color: None,
        }
    }

    pub fn with_accent(mut self, color: nih_plug_egui::egui::Color32) -> Self {
        self.accent_color = Some(color);
        self
    }
}

impl<'a, P: Param> Widget for Knob303<'a, P> {
    fn ui(self, ui: &mut Ui) -> Response {
        let desired_size = vec2(self.radius * 2.0 + 16.0, self.radius * 2.0 + 38.0);
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
            let is_hovered = response.hovered();

            // 1. Background disc (flat well)
            painter.circle_filled(center, self.radius, theme::WELL_BG);
            painter.circle_stroke(
                center,
                self.radius,
                Stroke::new(
                    1.0_f32,
                    if is_hovered {
                        theme::BORDER_FOCUS
                    } else {
                        theme::BORDER_DEFAULT
                    },
                ),
            );

            // 2. 270° Perimeter Track (135° to 405°)
            let start_angle = 135.0 * PI / 180.0;
            let end_angle = 405.0 * PI / 180.0;
            let arc_r = self.radius * 0.75;

            // Background inactive arc (24 segments, 0 heap allocations)
            const ARC_SEGMENTS: usize = 24;
            let mut prev_bg_pt = pos2(
                center.x + start_angle.cos() * arc_r,
                center.y + start_angle.sin() * arc_r,
            );
            for i in 1..=ARC_SEGMENTS {
                let t = i as f32 / ARC_SEGMENTS as f32;
                let a = start_angle + t * (end_angle - start_angle);
                let cur_pt = pos2(center.x + a.cos() * arc_r, center.y + a.sin() * arc_r);
                painter.line_segment([prev_bg_pt, cur_pt], Stroke::new(2.0_f32, theme::SURFACE_ELEVATED));
                prev_bg_pt = cur_pt;
            }

            // Radial Tick Marks (7 discrete ticks along the 270° sweep)
            let is_enabled = ui.is_enabled();
            const NUM_TICKS: usize = 7;
            let tick_r_inner = self.radius - 1.0;
            let tick_r_outer = self.radius + 1.5;
            let tick_color = if is_enabled { theme::BORDER_DEFAULT } else { theme::WELL_BG };
            for i in 0..NUM_TICKS {
                let t = i as f32 / (NUM_TICKS - 1) as f32;
                let a = start_angle + t * (end_angle - start_angle);
                let p1 = pos2(center.x + a.cos() * tick_r_inner, center.y + a.sin() * tick_r_inner);
                let p2 = pos2(center.x + a.cos() * tick_r_outer, center.y + a.sin() * tick_r_outer);
                painter.line_segment([p1, p2], Stroke::new(1.0_f32, tick_color));
            }

            // Active value arc
            let accent = if is_enabled {
                self.accent_color.unwrap_or(theme::ACCENT_ORANGE)
            } else {
                theme::TEXT_MUTED
            };
            if normalized > 0.001 {
                let mut prev_active_pt = pos2(
                    center.x + start_angle.cos() * arc_r,
                    center.y + start_angle.sin() * arc_r,
                );
                for i in 1..=ARC_SEGMENTS {
                    let t = i as f32 / ARC_SEGMENTS as f32;
                    let a = start_angle + t * normalized * (end_angle - start_angle);
                    let cur_pt = pos2(center.x + a.cos() * arc_r, center.y + a.sin() * arc_r);
                    painter.line_segment([prev_active_pt, cur_pt], Stroke::new(2.5_f32, accent));
                    prev_active_pt = cur_pt;
                }
            }

            // 3. Pointer Needle
            let current_angle = start_angle + normalized * (end_angle - start_angle);
            let needle_inner = pos2(
                center.x + current_angle.cos() * (self.radius * 0.2),
                center.y + current_angle.sin() * (self.radius * 0.2),
            );
            let needle_outer = pos2(
                center.x + current_angle.cos() * (self.radius * 0.75),
                center.y + current_angle.sin() * (self.radius * 0.75),
            );
            let needle_color = if is_enabled { theme::TEXT_PRIMARY } else { theme::TEXT_MUTED };
            painter.line_segment([needle_inner, needle_outer], Stroke::new(1.5_f32, needle_color));

            // 4. Parameter Label
            let label_y = center.y + self.radius + 6.0;
            let label_color = if is_enabled { theme::TEXT_SECONDARY } else { theme::TEXT_MUTED };
            painter.text(
                pos2(rect.center().x, label_y),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                self.label.to_uppercase(),
                FontId::new(8.5, FontFamily::Proportional),
                label_color,
            );

            // 5. Monospace Value Badge
            let value_y = label_y + 12.0;
            let raw_str = self.param.to_string();
            let clean_str = raw_str.split_whitespace().next().unwrap_or(&raw_str);
            let value_str = if let Ok(f) = clean_str.parse::<f32>() {
                if self.label.contains("CUTOFF") {
                    let hz = 300.0 * (15000.0 / 300.0_f32).powf(f.clamp(0.0, 1.0));
                    if hz >= 1000.0 {
                        format!("{:.1} kHz", hz / 1000.0)
                    } else {
                        format!("{:.0} Hz", hz)
                    }
                } else if self.label.contains("VOLUME") {
                    if f.abs() < 0.05 {
                        "+0.0 dB".to_string()
                    } else {
                        format!("{:+.1} dB", f)
                    }
                } else if self.label.contains("DRIFT") || self.label.contains("RECAL") {
                    let pct = if f <= 1.0 { f * 100.0 } else { f };
                    format!("{:.0} %", pct)
                } else if self.label.contains("TONE")
                    || self.label.contains("THRESH")
                    || self.label.contains("BASS")
                    || self.label.contains("RESONANCE")
                    || self.label.contains("ENV MOD")
                    || self.label.contains("ACCENT")
                    || self.label.contains("MIX")
                    || self.label.contains("DRIVE")
                    || self.label.contains("RESO")
                    || self.label.contains("PW")
                    || self.label.contains("SUB")
                    || self.label.contains("SWEEP")
                    || self.label.contains("VOL")
                    || self.label.contains("AGE")
                    || self.label.contains("DYN")
                    || self.label.contains("R-COMP")
                {
                    // Parameters with .with_value_to_string using "%" format (e.g. Clipper Drive) pass f as raw percentage value (e.g. 50.0), so avoid double multiply.
                    if f > 2.0 {
                        format!("{:.0} %", f)
                    } else {
                        format!("{:.0} %", f * 100.0)
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
                    if f.abs() < 0.05 {
                        "+0.0 ct".to_string()
                    } else {
                        format!("{:+.1} ct", f)
                    }
                } else if self.label.contains("PITCH") || self.label.contains("DET") {
                    if f.abs() < 0.05 {
                        "+0.0 st".to_string()
                    } else {
                        format!("{:+.1} st", f)
                    }
                } else if self.label.contains("IN") || self.label.contains("OUT") || self.label.contains("TILT") {
                    if f.abs() < 0.05 {
                        "+0.0 dB".to_string()
                    } else {
                        format!("{:+.1} dB", f)
                    }
                } else if self.label.contains("TEMPO") {
                    format!("{:.0} BPM", f)
                } else if f.abs() >= 1.0 {
                    format!("{:.0}", f)
                } else {
                    format!("{:.2}", f)
                }
            } else {
                raw_str
            };

            let pill_rect = Rect::from_center_size(pos2(rect.center().x, value_y), vec2(desired_size.x - 4.0, 12.0));
            painter.rect_filled(pill_rect, CornerRadius::same(2), theme::WELL_BG);
            painter.rect_stroke(pill_rect, CornerRadius::same(2), Stroke::new(1.0_f32, theme::BORDER_SUBTLE), nih_plug_egui::egui::StrokeKind::Inside);
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
