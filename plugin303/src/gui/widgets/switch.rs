use nih_plug::prelude::{EnumParam, ParamSetter};
use nih_plug_egui::egui::{
    pos2, vec2, Color32, FontFamily, FontId, Rect, Response, Sense, Stroke, StrokeKind, Ui, Widget,
};

use crate::gui::theme;

#[derive(Clone, Copy)]
struct ThumbColors {
    body: Color32,
    stroke: Color32,
    top_highlight: Option<Color32>,
    rib: Color32,
    active_text: Color32,
}

impl ThumbColors {
    fn new(thumb_color: Option<Color32>, active_text_color: Option<Color32>) -> Self {
        let mut tc = match thumb_color {
            Some(base) => {
                let r = base.r() as f32;
                let g = base.g() as f32;
                let b = base.b() as f32;

                let stroke = Color32::from_rgb(
                    (r * 0.65) as u8,
                    (g * 0.65) as u8,
                    (b * 0.65) as u8,
                );

                let top_highlight = Some(Color32::from_rgb(
                    ((r * 1.28) + 20.0).min(255.0) as u8,
                    ((g * 1.28) + 34.0).min(255.0) as u8,
                    ((b * 1.28) + 34.0).min(255.0) as u8,
                ));

                let rib = Color32::from_rgb(
                    (r * 0.53) as u8,
                    (g * 0.53) as u8,
                    (b * 0.53) as u8,
                );

                let active_text = Color32::from_rgb(
                    ((r * 1.84) + 74.0).min(255.0) as u8,
                    ((g * 1.84) + 74.0).min(255.0) as u8,
                    ((b * 1.84) + 74.0).min(255.0) as u8,
                );

                Self {
                    body: base,
                    stroke,
                    top_highlight,
                    rib,
                    active_text,
                }
            }
            None => Self {
                body: theme::KNOB_BODY_LIGHT,
                stroke: theme::KNOB_BEZEL,
                top_highlight: None,
                rib: Color32::from_rgb(0x70, 0x75, 0x82),
                active_text: theme::ACCENT_AMBER,
            },
        };
        if let Some(atc) = active_text_color {
            tc.active_text = atc;
        }
        tc
    }
}

fn draw_thumb_carriage(
    painter: &nih_plug_egui::egui::Painter,
    thumb_rect: Rect,
    colors: &ThumbColors,
    is_vertical: bool,
) {
    painter.rect_filled(thumb_rect, 2.0, colors.body);
    if let Some(highlight_color) = colors.top_highlight {
        // Brushed metallic top highlight
        painter.line_segment(
            [
                pos2(thumb_rect.left() + 1.5, thumb_rect.top() + 1.0),
                pos2(thumb_rect.right() - 1.5, thumb_rect.top() + 1.0),
            ],
            Stroke::new(1.0_f32, highlight_color),
        );
    }
    painter.rect_stroke(
        thumb_rect,
        2.0,
        Stroke::new(1.2_f32, colors.stroke),
        StrokeKind::Inside,
    );

    let num_ribs = 3;
    if is_vertical {
        // Horizontal knurling ribs on vertical thumb
        for r in 0..num_ribs {
            let rib_offset = (r as f32 - (num_ribs - 1) as f32 / 2.0) * 2.5;
            let ry = thumb_rect.center().y + rib_offset;
            painter.line_segment(
                [
                    pos2(thumb_rect.left() + 2.0, ry),
                    pos2(thumb_rect.right() - 2.0, ry),
                ],
                Stroke::new(1.0_f32, colors.rib),
            );
        }
    } else {
        // Vertical knurling ribs on horizontal thumb
        for r in 0..num_ribs {
            let rib_offset = (r as f32 - (num_ribs - 1) as f32 / 2.0) * 3.0;
            let rx = thumb_rect.center().x + rib_offset;
            painter.line_segment(
                [
                    pos2(rx, thumb_rect.top() + 3.0),
                    pos2(rx, thumb_rect.bottom() - 3.0),
                ],
                Stroke::new(1.0_f32, colors.rib),
            );
        }
    }
}

pub struct SlideSwitch<'a, T: nih_plug::prelude::Enum + PartialEq + 'static> {
    param: &'a EnumParam<T>,
    setter: &'a ParamSetter<'a>,
    label: &'a str,
    options: &'a [&'a str],
    thumb_color: Option<Color32>,
    active_text_color: Option<Color32>,
    is_vertical: bool,
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
            thumb_color: None,
            active_text_color: None,
            is_vertical: false,
        }
    }

    pub fn with_thumb_color(mut self, color: Color32) -> Self {
        self.thumb_color = Some(color);
        self
    }

    pub fn with_active_text_color(mut self, color: Color32) -> Self {
        self.active_text_color = Some(color);
        self
    }

    pub fn vertical(mut self) -> Self {
        self.is_vertical = true;
        self
    }
}

impl<'a, T: nih_plug::prelude::Enum + PartialEq + 'static> Widget for SlideSwitch<'a, T> {
    fn ui(self, ui: &mut Ui) -> Response {
        let num_opts = self.options.len().max(1);
        let opt_font = FontId::new(8.5, FontFamily::Proportional);
        let colors = ThumbColors::new(self.thumb_color, self.active_text_color);

        if self.is_vertical {
            let max_text_width = self
                .options
                .iter()
                .map(|opt| {
                    ui.fonts(|f| {
                        f.layout_no_wrap(opt.to_string(), opt_font.clone(), Color32::WHITE)
                            .size()
                            .x
                    })
                })
                .fold(0.0f32, f32::max);
            let slot_height = 16.0_f32;
            let track_height = slot_height * num_opts as f32;
            let width = 24.0 + max_text_width + 12.0;
            let height = track_height + 22.0;
            let (rect, mut response) =
                ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());

            let current_idx = self.param.value().to_index();
            let track_rect = Rect::from_min_size(
                pos2(rect.left() + 4.0, rect.top() + 16.0),
                vec2(12.0, track_height),
            );

            if response.clicked() || response.dragged() {
                if let Some(mouse_pos) = response.interact_pointer_pos() {
                    let rel_y = (mouse_pos.y - track_rect.top()).clamp(0.0, track_rect.height());
                    let target_idx = ((rel_y / slot_height).floor() as usize).min(num_opts - 1);
                    if target_idx != current_idx || response.clicked() {
                        self.setter.begin_set_parameter(self.param);
                        self.setter.set_parameter_normalized(
                            self.param,
                            target_idx as f32 / (num_opts - 1).max(1) as f32,
                        );
                        self.setter.end_set_parameter(self.param);
                        response.mark_changed();
                    }
                }
            }

            if ui.is_rect_visible(rect) {
                let painter = ui.painter();

                // Main parameter title
                painter.text(
                    pos2(rect.left() + 2.0, rect.top() + 4.0),
                    nih_plug_egui::egui::Align2::LEFT_TOP,
                    self.label,
                    FontId::new(9.0, FontFamily::Proportional),
                    theme::TEXT_SECONDARY,
                );

                // Recessed vertical track
                painter.rect_filled(track_rect, 2.0, Color32::from_rgb(0x10, 0x12, 0x16));
                painter.rect_stroke(
                    track_rect,
                    2.0,
                    Stroke::new(1.0_f32, theme::PANEL_BORDER),
                    StrokeKind::Inside,
                );

                // Detent tick marks
                for i in 0..num_opts {
                    let detent_y = track_rect.top() + slot_height * (i as f32 + 0.5);
                    painter.line_segment(
                        [
                            pos2(track_rect.left() + 2.0, detent_y),
                            pos2(track_rect.right() - 2.0, detent_y),
                        ],
                        Stroke::new(1.0_f32, Color32::from_rgb(0x28, 0x2c, 0x36)),
                    );
                }

                // Sliding thumb carriage
                let thumb_y = track_rect.top() + slot_height * (current_idx as f32 + 0.5);
                let thumb_rect = Rect::from_center_size(
                    pos2(track_rect.center().x, thumb_y),
                    vec2(16.0, 12.0),
                );

                draw_thumb_carriage(painter, thumb_rect, &colors, true);

                // Option labels stacked vertically to the right of each detent
                for (i, opt) in self.options.iter().enumerate() {
                    let detent_y = track_rect.top() + slot_height * (i as f32 + 0.5);
                    let is_selected = i == current_idx;
                    let text_color = if is_selected {
                        colors.active_text
                    } else {
                        theme::TEXT_MUTED
                    };
                    painter.text(
                        pos2(track_rect.right() + 8.0, detent_y - 1.5),
                        nih_plug_egui::egui::Align2::LEFT_CENTER,
                        *opt,
                        FontId::new(8.0, FontFamily::Proportional),
                        text_color,
                    );
                }
            }

            return response;
        }

        let max_text_width = self
            .options
            .iter()
            .map(|opt| {
                ui.fonts(|f| {
                    f.layout_no_wrap(opt.to_string(), opt_font.clone(), Color32::WHITE)
                        .size()
                        .x
                })
            })
            .fold(0.0f32, f32::max);
        let per_slot = (max_text_width + 12.0).max(30.0);
        let width = per_slot * num_opts as f32 + 8.0;
        let height = 44.0;
        let (rect, mut response) =
            ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());

        let current_idx = self.param.value().to_index();

        let track_rect = Rect::from_min_size(
            pos2(rect.left() + 4.0, rect.top() + 15.0),
            vec2(width - 8.0, 12.0),
        );
        let slot_width = (width - 8.0) / num_opts as f32;

        if response.clicked() {
            if let Some(mouse_pos) = response.interact_pointer_pos() {
                let rel_x = (mouse_pos.x - track_rect.left()).clamp(0.0, track_rect.width());
                let target_idx = ((rel_x / slot_width).floor() as usize).min(num_opts - 1);
                self.setter.begin_set_parameter(self.param);
                self.setter.set_parameter_normalized(
                    self.param,
                    target_idx as f32 / (num_opts - 1).max(1) as f32,
                );
                self.setter.end_set_parameter(self.param);
                response.mark_changed();
            }
        } else if response.dragged() {
            if let Some(mouse_pos) = response.interact_pointer_pos() {
                let rel_x = (mouse_pos.x - track_rect.left()).clamp(0.0, track_rect.width());
                let target_idx = ((rel_x / slot_width).floor() as usize).min(num_opts - 1);
                if target_idx != current_idx {
                    self.setter.begin_set_parameter(self.param);
                    self.setter.set_parameter_normalized(
                        self.param,
                        target_idx as f32 / (num_opts - 1).max(1) as f32,
                    );
                    self.setter.end_set_parameter(self.param);
                    response.mark_changed();
                }
            }
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();

            // Main parameter title
            painter.text(
                pos2(rect.center().x, rect.top() + 4.0),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                self.label,
                FontId::new(9.5, FontFamily::Proportional),
                theme::TEXT_SECONDARY,
            );

            // Recessed track
            painter.rect_filled(track_rect, 2.0, Color32::from_rgb(0x10, 0x12, 0x16));
            painter.rect_stroke(
                track_rect,
                2.0,
                Stroke::new(1.0_f32, theme::PANEL_BORDER),
                StrokeKind::Inside,
            );

            // Detent tick marks
            for i in 0..num_opts {
                let detent_x = track_rect.left() + slot_width * (i as f32 + 0.5);
                painter.line_segment(
                    [
                        pos2(detent_x, track_rect.top() + 2.0),
                        pos2(detent_x, track_rect.bottom() - 2.0),
                    ],
                    Stroke::new(1.0_f32, Color32::from_rgb(0x28, 0x2c, 0x36)),
                );
            }

            // Knurled silver sliding carriage (thumb)
            let thumb_x = track_rect.left() + slot_width * (current_idx as f32 + 0.5);
            let thumb_w = (slot_width * 0.75).clamp(12.0, 22.0);
            let thumb_rect = Rect::from_center_size(
                pos2(thumb_x, track_rect.center().y),
                vec2(thumb_w, 16.0),
            );

            draw_thumb_carriage(painter, thumb_rect, &colors, false);

            // Option labels positioned below the track at each detent
            for (i, opt) in self.options.iter().enumerate() {
                let opt_x = track_rect.left() + slot_width * (i as f32 + 0.5);
                let is_selected = i == current_idx;
                let text_color = if is_selected {
                    colors.active_text
                } else {
                    theme::TEXT_MUTED
                };
                painter.text(
                    pos2(opt_x, rect.bottom() - 4.0),
                    nih_plug_egui::egui::Align2::CENTER_CENTER,
                    *opt,
                    FontId::new(8.0, FontFamily::Proportional),
                    text_color,
                );
            }
        }

        response
    }
}
