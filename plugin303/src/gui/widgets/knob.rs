use nih_plug::prelude::{Param, ParamSetter};
use nih_plug_egui::egui::epaint::Mesh;
use nih_plug_egui::egui::{pos2, vec2, Color32, FontFamily, FontId, Pos2, Response, Sense, Stroke, Ui, Widget};
use std::f32::consts::PI;
use std::f32::consts::TAU;

use crate::gui::theme;

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
    )
}

/// Filled circle with a vertical linear gradient (light top, dark bottom),
/// built as a triangle-fan mesh. Reads as brushed aluminum under panel light.
fn gradient_circle(
    painter: &nih_plug_egui::egui::Painter,
    center: Pos2,
    radius: f32,
    top: Color32,
    bottom: Color32,
) {
    const SEGMENTS: usize = 48;
    let mut mesh = Mesh::default();
    let center_idx = mesh.vertices.len() as u32;
    mesh.colored_vertex(center, lerp_color(top, bottom, 0.5));
    for i in 0..=SEGMENTS {
        let a = i as f32 / SEGMENTS as f32 * TAU;
        let t = a.sin() * 0.5 + 0.5;
        mesh.colored_vertex(
            pos2(center.x + a.cos() * radius, center.y + a.sin() * radius),
            lerp_color(top, bottom, t),
        );
    }
    for i in 0..SEGMENTS {
        let base = center_idx + 1;
        mesh.add_triangle(center_idx, base + i as u32, base + i as u32 + 1);
    }
    painter.add(nih_plug_egui::egui::Shape::mesh(mesh));
}

/// Bevel highlight: bright arc across the top-left rim, shadow arc bottom-right.
fn bevel_ring(painter: &nih_plug_egui::egui::Painter, center: Pos2, radius: f32) {
    let r = radius - 0.8;
    let highlight: Vec<_> = (0..=16)
        .map(|i| {
            let a = (160.0 - i as f32 * (140.0 / 16.0)) * PI / 180.0;
            pos2(center.x + a.cos() * r, center.y - a.sin() * r)
        })
        .collect();
    painter.add(nih_plug_egui::egui::Shape::line(
        highlight,
        Stroke::new(1.2_f32, Color32::from_rgba_unmultiplied(0xff, 0xff, 0xff, 120)),
    ));
    let shadow: Vec<_> = (0..=16)
        .map(|i| {
            let a = (200.0 + i as f32 * (140.0 / 16.0)) * PI / 180.0;
            pos2(center.x + a.cos() * r, center.y - a.sin() * r)
        })
        .collect();
    painter.add(nih_plug_egui::egui::Shape::line(
        shadow,
        Stroke::new(1.2_f32, Color32::from_rgba_unmultiplied(0x00, 0x00, 0x00, 110)),
    ));
}

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
        let desired_size = vec2(self.radius * 2.0 + 20.0, self.radius * 2.0 + 40.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());

        let center = pos2(rect.center().x, rect.top() + self.radius + 11.0);
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

            // Discrete radial tick marks around bezel
            let num_ticks = 11;
            for i in 0..num_ticks {
                let frac = i as f32 / (num_ticks - 1) as f32;
                let tick_angle = start_angle + frac * (end_angle - start_angle);
                let is_major = i == 0 || i == (num_ticks - 1) || i == (num_ticks / 2);
                let r_inner = self.radius + 6.0;
                let r_outer = if is_major { self.radius + 9.5 } else { self.radius + 8.0 };
                let p_in = pos2(center.x + tick_angle.cos() * r_inner, center.y + tick_angle.sin() * r_inner);
                let p_out = pos2(center.x + tick_angle.cos() * r_outer, center.y + tick_angle.sin() * r_outer);
                let alpha = if is_major { 180 } else { 130 };
                let tick_color = nih_plug_egui::egui::Color32::from_rgba_unmultiplied(0xe2, 0xe8, 0xf0, alpha);
                let tick_stroke_width = 1.0_f32;
                painter.line_segment([p_in, p_out], Stroke::new(tick_stroke_width, tick_color));
            }

            // Active metallic amber arc around bezel
            let arc_r = self.radius + 2.5;
            if normalized > 0.001 {
                let arc_points: Vec<_> = (0..=24)
                    .map(|i| {
                        let t = i as f32 / 24.0;
                        let a = start_angle + t * normalized * (end_angle - start_angle);
                        pos2(center.x + a.cos() * arc_r, center.y + a.sin() * arc_r)
                    })
                    .collect();
                painter.add(nih_plug_egui::egui::Shape::line(
                    arc_points,
                    Stroke::new(1.8_f32, theme::SOUPED_ORANGE),
                ));
            }

            // Brushed aluminum dial body: vertical gradient face + inner cap + bevel ring
            gradient_circle(
                painter,
                center,
                self.radius,
                theme::KNOB_BODY_LIGHT,
                theme::KNOB_BODY_DARK,
            );
            gradient_circle(
                painter,
                center,
                self.radius * 0.85,
                lerp_color(theme::KNOB_BODY_DARK, theme::KNOB_BODY_LIGHT, 0.7),
                lerp_color(theme::KNOB_BODY_DARK, Color32::BLACK, 0.15),
            );
            bevel_ring(painter, center, self.radius);
            painter.circle_stroke(
                center,
                self.radius,
                Stroke::new(1.2_f32, theme::PANEL_BORDER_HIGHLIGHT),
            );

            // Engraved metallic silver/white pointer line
            let pointer_len = self.radius * 0.85;
            let pointer_end = pos2(
                center.x + current_angle.cos() * pointer_len,
                center.y + current_angle.sin() * pointer_len,
            );
            let pointer_start = pos2(
                center.x + current_angle.cos() * (self.radius * 0.15),
                center.y + current_angle.sin() * (self.radius * 0.15),
            );
            // Engraved pointer: dark groove with a thin bright catch-light core
            painter.line_segment(
                [pointer_start, pointer_end],
                Stroke::new(2.6_f32, nih_plug_egui::egui::Color32::from_rgb(0x20, 0x22, 0x26)),
            );
            painter.line_segment(
                [pointer_start, pointer_end],
                Stroke::new(1.2_f32, nih_plug_egui::egui::Color32::from_rgb(0xe2, 0xe8, 0xf0)),
            );

            // Parameter Title Label below dial
            let label_y = center.y + self.radius + 10.0;
            let label_pos = pos2(rect.center().x, label_y);
            painter.text(
                label_pos,
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                self.label.to_uppercase(),
                FontId::new(9.0, FontFamily::Proportional),
                theme::TEXT_PRIMARY,
            );

            // Human-readable formatted value readout below title (with physical units)
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
                    // Gain param: plain value is dB, not 0..1 — format before the
                    // percent branch can claim it.
                    if f.abs() < 0.05 {
                        "+0.0 dB".to_string()
                    } else {
                        format!("{:+.1} dB", f)
                    }
                } else if raw_str.contains("ohm") {
                    // Clipper resistances: format as perceptual percentage across pot range.
                    if self.label.contains("DRIVE") {
                        let pct = ((f - 22_000.0) / (100_000.0 - 22_000.0) * 100.0).clamp(0.0, 100.0);
                        format!("{:.0}%", pct)
                    } else if self.label.contains("THRESH") {
                        let pct = ((f - 2_200.0) / (22_000.0 - 2_200.0) * 100.0).clamp(0.0, 100.0);
                        format!("{:.0}%", pct)
                    } else if self.label.contains("BASS") {
                        let pct = ((f - 4_700.0) / (22_000.0 - 4_700.0) * 100.0).clamp(0.0, 100.0);
                        format!("{:.0}%", pct)
                    } else if f >= 1_000.0 {
                        format!("{:.1}kΩ", f / 1000.0)
                    } else {
                        format!("{:.0}Ω", f)
                    }
                } else if self.label.contains("RESONANCE")
                    || self.label.contains("ENV MOD")
                    || self.label.contains("ACCENT")
                    || self.label.contains("MIX")
                    || self.label.contains("DRIVE")
                    || self.label.contains("RESO")
                    || self.label.contains("PW")
                    || self.label.contains("SUB")
                    || self.label.contains("SWEEP")
                    || self.label.contains("VOL")
                    || self.label.contains("DRIFT")
                    || self.label.contains("AGE")
                    || self.label.contains("DYN")
                {
                    if (0.0..=1.0).contains(&f) {
                        format!("{:.0}%", f * 100.0)
                    } else if f.abs() >= 100.0 {
                        format!("{:.0}", f)
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
                    if f.abs() < 0.01 {
                        "+0 ct".to_string()
                    } else {
                        format!("{:+.0} ct", f)
                    }
                } else if self.label.contains("PITCH") || self.label.contains("DET") {
                    if f.abs() < 0.01 {
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
                } else if self.label.contains("TONE") {
                    format!("{:.2}", f)
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
                nih_plug_egui::egui::Color32::from_rgb(0xd0, 0xd8, 0xe0),
            );
        }

        response
    }
}
