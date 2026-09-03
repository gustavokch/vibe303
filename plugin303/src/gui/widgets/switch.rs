use nih_plug::prelude::{EnumParam, ParamSetter};
use nih_plug_egui::egui::{
    pos2, vec2, Color32, CornerRadius, FontFamily, FontId, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, Widget,
};

use crate::gui::theme;

pub struct SlideSwitch<'a, T: nih_plug::prelude::Enum + PartialEq + 'static> {
    param: &'a EnumParam<T>,
    setter: &'a ParamSetter<'a>,
    label: &'a str,
    options: &'a [&'a str],
    thumb_color: Option<Color32>,
    active_text_color: Option<Color32>,
    is_vertical: bool,
    fixed_width: Option<f32>,
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
            fixed_width: None,
        }
    }

    /// Stretch the horizontal track to `width` instead of hugging the option
    /// labels — used when a switch must fill a layout column so no dead band
    /// remains beside it (round-13 issue 5).
    pub fn with_width(mut self, width: f32) -> Self {
        self.fixed_width = Some(width);
        self
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
        let accent = self.active_text_color.unwrap_or(theme::ACCENT_ORANGE);

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
            let slot_height = 20.0_f32;
            let track_height = slot_height * num_opts as f32;
            let width = (20.0 + max_text_width + 16.0).max(52.0);
            let height = track_height + 16.0;
            let (rect, mut response) =
                ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());

            let current_idx = self.param.value().to_index();
            let track_rect = Rect::from_min_size(
                pos2(rect.left() + 2.0, rect.top() + 14.0),
                vec2(width - 4.0, track_height),
            );

            if response.clicked() || response.dragged() {
                if let Some(mouse_pos) = response.interact_pointer_pos() {
                    let rel_y =
                        (mouse_pos.y - track_rect.top()).clamp(0.0, track_rect.height() - 0.001);
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
                let is_enabled = ui.is_enabled();
                let switch_accent = if is_enabled {
                    accent
                } else {
                    theme::TEXT_MUTED
                };

                // Main parameter title
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

                // Recessed full-width vertical container
                painter.rect_filled(track_rect, CornerRadius::same(2), theme::WELL_BG);
                painter.rect_stroke(
                    track_rect,
                    CornerRadius::same(2),
                    Stroke::new(1.0_f32, theme::BORDER_DEFAULT),
                    StrokeKind::Inside,
                );

                // Sliding active segmented pill
                let thumb_y = track_rect.top() + slot_height * current_idx as f32;
                let thumb_rect = Rect::from_min_size(
                    pos2(track_rect.left() + 1.0, thumb_y + 1.0),
                    vec2(track_rect.width() - 2.0, slot_height - 2.0),
                );
                painter.rect_filled(
                    thumb_rect,
                    CornerRadius::same(2),
                    if is_enabled {
                        theme::SURFACE_ELEVATED
                    } else {
                        theme::WELL_BG
                    },
                );
                painter.rect_stroke(
                    thumb_rect,
                    CornerRadius::same(2),
                    Stroke::new(
                        1.0_f32,
                        if is_enabled {
                            switch_accent
                        } else {
                            theme::BORDER_SUBTLE
                        },
                    ),
                    StrokeKind::Inside,
                );

                // Options with integrated glyphs & labels
                for (i, opt) in self.options.iter().enumerate() {
                    let row_y = track_rect.top() + slot_height * i as f32;
                    let cy = row_y + slot_height * 0.5;
                    let is_selected = i == current_idx;
                    let text_color = if !is_enabled {
                        theme::TEXT_MUTED
                    } else if is_selected {
                        theme::TEXT_PRIMARY
                    } else {
                        theme::TEXT_MUTED
                    };
                    let glyph_color = if !is_enabled {
                        theme::TEXT_MUTED
                    } else if is_selected {
                        switch_accent
                    } else {
                        theme::TEXT_MUTED
                    };

                    // Waveform glyph
                    let gx = track_rect.left() + 9.0;
                    let stroke = Stroke::new(1.2_f32, glyph_color);
                    match i {
                        0 => {
                            // Saw
                            painter.line_segment(
                                [pos2(gx - 4.0, cy + 3.0), pos2(gx + 3.0, cy - 3.0)],
                                stroke,
                            );
                            painter.line_segment(
                                [pos2(gx + 3.0, cy - 3.0), pos2(gx + 3.0, cy + 3.0)],
                                stroke,
                            );
                        }
                        1 => {
                            // Square
                            painter.line_segment(
                                [pos2(gx - 4.0, cy + 3.0), pos2(gx - 4.0, cy - 3.0)],
                                stroke,
                            );
                            painter.line_segment(
                                [pos2(gx - 4.0, cy - 3.0), pos2(gx, cy - 3.0)],
                                stroke,
                            );
                            painter.line_segment([pos2(gx, cy - 3.0), pos2(gx, cy + 3.0)], stroke);
                            painter.line_segment(
                                [pos2(gx, cy + 3.0), pos2(gx + 4.0, cy + 3.0)],
                                stroke,
                            );
                        }
                        2 => {
                            // Sweep
                            painter.line_segment(
                                [pos2(gx - 4.0, cy + 3.0), pos2(gx - 2.0, cy - 3.0)],
                                stroke,
                            );
                            painter.line_segment(
                                [pos2(gx - 2.0, cy - 3.0), pos2(gx + 2.0, cy + 3.0)],
                                stroke,
                            );
                            painter.line_segment(
                                [pos2(gx + 2.0, cy + 3.0), pos2(gx + 4.0, cy - 3.0)],
                                stroke,
                            );
                        }
                        _ => {}
                    }

                    // Label text
                    painter.text(
                        pos2(track_rect.left() + 20.0, cy),
                        nih_plug_egui::egui::Align2::LEFT_CENTER,
                        *opt,
                        FontId::new(8.5, FontFamily::Proportional),
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
        let width = self
            .fixed_width
            .unwrap_or(per_slot * num_opts as f32 + 4.0)
            .max(per_slot + 4.0);
        let height = 30.0;
        let (rect, mut response) =
            ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());

        let current_idx = self.param.value().to_index();
        let track_rect = Rect::from_min_size(
            pos2(rect.left() + 2.0, rect.top() + 13.0),
            vec2(width - 4.0, 15.0),
        );
        let slot_width = (width - 4.0) / num_opts as f32;

        if response.clicked() || response.dragged() {
            if let Some(mouse_pos) = response.interact_pointer_pos() {
                let rel_x = (mouse_pos.x - track_rect.left()).clamp(0.0, track_rect.width());
                let target_idx = ((rel_x / slot_width).floor() as usize).min(num_opts - 1);
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
            let is_enabled = ui.is_enabled();
            let switch_accent = if is_enabled {
                accent
            } else {
                theme::TEXT_MUTED
            };

            // Main parameter title
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

            // Recessed track container
            painter.rect_filled(track_rect, CornerRadius::same(2), theme::WELL_BG);
            painter.rect_stroke(
                track_rect,
                CornerRadius::same(2),
                Stroke::new(1.0_f32, theme::BORDER_DEFAULT),
                StrokeKind::Inside,
            );

            // Active segmented pill
            let thumb_x = track_rect.left() + slot_width * current_idx as f32;
            let thumb_rect = Rect::from_min_size(
                pos2(thumb_x + 1.0, track_rect.top() + 1.0),
                vec2(slot_width - 2.0, track_rect.height() - 2.0),
            );
            painter.rect_filled(
                thumb_rect,
                CornerRadius::same(2),
                if is_enabled {
                    theme::SURFACE_ELEVATED
                } else {
                    theme::WELL_BG
                },
            );
            painter.rect_stroke(
                thumb_rect,
                CornerRadius::same(2),
                Stroke::new(
                    1.0_f32,
                    if is_enabled {
                        switch_accent
                    } else {
                        theme::BORDER_SUBTLE
                    },
                ),
                StrokeKind::Inside,
            );

            // Option labels
            for (i, opt) in self.options.iter().enumerate() {
                let opt_x = track_rect.left() + slot_width * (i as f32 + 0.5);
                let is_selected = i == current_idx;
                let text_color = if !is_enabled {
                    theme::TEXT_MUTED
                } else if is_selected {
                    theme::TEXT_PRIMARY
                } else {
                    theme::TEXT_MUTED
                };
                painter.text(
                    pos2(opt_x, track_rect.center().y),
                    nih_plug_egui::egui::Align2::CENTER_CENTER,
                    *opt,
                    FontId::new(8.5, FontFamily::Proportional),
                    text_color,
                );
            }
        }

        response
    }
}
