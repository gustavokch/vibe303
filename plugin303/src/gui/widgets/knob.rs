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
        let desired_size = vec2(self.radius * 2.0 + 16.0, self.radius * 2.0 + 34.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());

        let center = pos2(rect.center().x, rect.top() + self.radius + 14.0);
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

            // Label above
            painter.text(
                pos2(rect.center().x, rect.top() + 4.0),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                self.label,
                FontId::new(10.0, FontFamily::Proportional),
                theme::TEXT_SECONDARY,
            );

            // Tick arc (270 degrees: 135 deg to 405 deg)
            let start_angle = 135.0 * PI / 180.0;
            let end_angle = 405.0 * PI / 180.0;
            let current_angle = start_angle + normalized * (end_angle - start_angle);

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
                center.x + current_angle.cos() * (self.radius * 0.2),
                center.y + current_angle.sin() * (self.radius * 0.2),
            );
            painter.line_segment(
                [pointer_start, pointer_end],
                Stroke::new(2.5_f32, theme::KNOB_POINTER),
            );

            // Formatted value readout below
            let value_str = self.param.to_string();
            painter.text(
                pos2(rect.center().x, rect.bottom() - 6.0),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                value_str,
                FontId::new(9.5, FontFamily::Monospace),
                theme::TEXT_PRIMARY,
            );
        }

        response
    }
}
