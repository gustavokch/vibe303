use nih_plug_egui::egui::{vec2, Color32, Response, Sense, Stroke, Ui, Widget};

pub struct LedIndicator {
    pub active: bool,
    pub color: Color32,
    pub glow_color: Color32,
    pub radius: f32,
}

impl LedIndicator {
    pub fn new(active: bool, color: Color32, glow_color: Color32, radius: f32) -> Self {
        Self {
            active,
            color,
            glow_color,
            radius,
        }
    }
}

impl Widget for LedIndicator {
    fn ui(self, ui: &mut Ui) -> Response {
        let size = vec2(self.radius * 2.0 + 4.0, self.radius * 2.0 + 4.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let center = rect.center();
            if self.active {
                // Outer glow bloom
                painter.circle_filled(center, self.radius * 2.0, self.glow_color);
                painter.circle_filled(center, self.radius, self.color);
            } else {
                painter.circle_filled(center, self.radius, Color32::from_rgb(0x28, 0x2e, 0x36));
                painter.circle_stroke(
                    center,
                    self.radius,
                    Stroke::new(1.0_f32, Color32::from_rgb(0x18, 0x1b, 0x20)),
                );
            }
        }
        response
    }
}
