use nih_plug::prelude::{EnumParam, ParamSetter};
use nih_plug_egui::egui::{
    pos2, vec2, Color32, FontFamily, FontId, Rect, Response, Sense, Stroke, StrokeKind, Ui, Widget,
};

use crate::gui::theme;

pub struct SlideSwitch<'a, T: nih_plug::prelude::Enum + PartialEq + 'static> {
    param: &'a EnumParam<T>,
    setter: &'a ParamSetter<'a>,
    label: &'a str,
    options: &'a [&'a str],
}

impl<'a, T: nih_plug::prelude::Enum + PartialEq + 'static> SlideSwitch<'a, T> {
    pub fn new(
        param: &'a EnumParam<T>,
        setter: &'a ParamSetter<'a>,
        label: &'a str,
        options: &'a [&'a str],
    ) -> Self {
        Self {
            param,
            setter,
            label,
            options,
        }
    }
}

impl<'a, T: nih_plug::prelude::Enum + PartialEq + 'static> Widget for SlideSwitch<'a, T> {
    fn ui(self, ui: &mut Ui) -> Response {
        let num_opts = self.options.len().max(1);
        let width = 28.0 * num_opts as f32;
        let height = 36.0;
        let (rect, mut response) =
            ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());

        let current_idx = self.param.value().to_index();

        if response.clicked() {
            let next_idx = (current_idx + 1) % num_opts;
            self.setter.begin_set_parameter(self.param);
            self.setter
                .set_parameter_normalized(self.param, next_idx as f32 / (num_opts - 1).max(1) as f32);
            self.setter.end_set_parameter(self.param);
            response.mark_changed();
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            painter.text(
                pos2(rect.center().x, rect.top() + 4.0),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                self.label,
                FontId::new(9.5, FontFamily::Proportional),
                theme::TEXT_SECONDARY,
            );

            let track_rect = Rect::from_min_size(
                pos2(rect.left() + 4.0, rect.top() + 16.0),
                vec2(width - 8.0, 16.0),
            );
            painter.rect_filled(track_rect, 3.0, Color32::from_rgb(0x10, 0x12, 0x16));
            painter.rect_stroke(track_rect, 3.0, Stroke::new(1.0_f32, theme::PANEL_BORDER), StrokeKind::Inside);

            let slot_width = (width - 8.0) / num_opts as f32;
            let thumb_x = track_rect.left() + slot_width * (current_idx as f32 + 0.5);
            let thumb_rect = Rect::from_center_size(
                pos2(thumb_x, track_rect.center().y),
                vec2(slot_width - 2.0, 14.0),
            );

            painter.rect_filled(thumb_rect, 2.0, theme::KNOB_BODY_LIGHT);
            painter.rect_stroke(thumb_rect, 2.0, Stroke::new(1.0_f32, theme::KNOB_BEZEL), StrokeKind::Inside);

            let opt_label = self.options.get(current_idx).unwrap_or(&"");
            painter.text(
                thumb_rect.center(),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                *opt_label,
                FontId::new(8.5, FontFamily::Proportional),
                Color32::BLACK,
            );
        }

        response
    }
}
