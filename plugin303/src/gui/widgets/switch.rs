use nih_plug::prelude::{EnumParam, ParamSetter};
use nih_plug_egui::egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, Response,
    Sense, Stroke, Ui, Widget,
};

use crate::gui::theme;

/// Segment height. One height for every segmented control keeps the rows they
/// sit in aligned.
const SEG_H: f32 = 18.0;
/// Hairline of panel colour between segments.
const SEG_GAP: f32 = 1.0;
/// Space the caption takes when it sits above the segments.
const CAPTION_H: f32 = 11.0;
/// Gap between a left-hand caption and its segments.
const CAPTION_GAP: f32 = 6.0;
const FONT_SIZE: f32 = 8.5;

/// Where a segmented control draws its caption.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Caption {
    /// Above the segments (knob-column contexts).
    Above,
    /// Left of the segments, vertically centred (header strips).
    Left,
}

/// Painter clone with egui's disabled fade removed.
///
/// Inside `add_enabled_ui(false, …)` the Ui's painter fades everything toward
/// the window fill, which compounded on the explicitly muted tokens below and
/// dropped the selected-segment mark to the track colour (round-14 issue 2).
/// One dimming mechanism only: the tokens.
fn fade_cleared_painter(ui: &Ui) -> Painter {
    let mut painter = ui.painter().clone();
    painter.set_fade_to_color(None);
    painter
}

fn text_width(ui: &Ui, text: &str) -> f32 {
    ui.fonts(|f| {
        f.layout_no_wrap(
            text.to_string(),
            FontId::new(FONT_SIZE, FontFamily::Proportional),
            Color32::WHITE,
        )
        .size()
        .x
    })
}

/// Corner radius for segment `i` of `n`: only the outer ends are rounded, so a
/// row of segments reads as one flat bar.
fn segment_corners(i: usize, n: usize, vertical: bool) -> CornerRadius {
    let r = theme::RADIUS;
    let (first, last) = (i == 0, i + 1 == n);
    if vertical {
        CornerRadius {
            nw: if first { r } else { 0 },
            ne: if first { r } else { 0 },
            sw: if last { r } else { 0 },
            se: if last { r } else { 0 },
        }
    } else {
        CornerRadius {
            nw: if first { r } else { 0 },
            sw: if first { r } else { 0 },
            ne: if last { r } else { 0 },
            se: if last { r } else { 0 },
        }
    }
}

/// Fill and text colour for one segment.
fn segment_colors(selected: bool, hovered: bool, enabled: bool) -> (Color32, Color32) {
    match (selected, enabled) {
        (true, true) => (theme::ACCENT_ORANGE, theme::TEXT_ON_ACCENT),
        // Disabled selection keeps its position without claiming the accent.
        (true, false) => (theme::SURFACE_HOVER, theme::TEXT_SECONDARY),
        (false, true) if hovered => (theme::SURFACE_HOVER, theme::TEXT_PRIMARY),
        (false, true) => (theme::SURFACE_ELEVATED, theme::TEXT_SECONDARY),
        // The well plane shows on the header strips where every disabled bar
        // sits; SURFACE_PRESSED was the strip's own colour (round-22 #10).
        (false, false) => (theme::WELL_BG, theme::TEXT_MUTED),
    }
}

/// Shared renderer for horizontal segmented parameter switches.
fn render_hsegment_row(
    ui: &mut Ui,
    label: &str,
    caption: Caption,
    options: &[&str],
    current_idx: usize,
    fixed_width: Option<f32>,
    on_select: &mut dyn FnMut(&mut Response, usize),
) -> Response {
    let num_opts = options.len().max(1);

    let max_text_width = options
        .iter()
        .map(|opt| text_width(ui, opt))
        .fold(0.0f32, f32::max);
    let per_slot = (max_text_width + 16.0).max(34.0);
    let seg_width = fixed_width
        .unwrap_or(per_slot * num_opts as f32 + SEG_GAP * (num_opts - 1) as f32)
        .max(per_slot);
    let caption_w = match caption {
        Caption::Left => text_width(ui, label) + CAPTION_GAP,
        Caption::Above => 0.0,
    };
    let caption_h = match caption {
        Caption::Above => CAPTION_H,
        Caption::Left => 0.0,
    };
    let (rect, mut response) = ui.allocate_exact_size(
        vec2(caption_w + seg_width, caption_h + SEG_H),
        Sense::click_and_drag(),
    );

    let track_rect = Rect::from_min_size(
        pos2(rect.left() + caption_w, rect.top() + caption_h),
        vec2(seg_width, SEG_H),
    );
    let slot_width = (seg_width - SEG_GAP * (num_opts - 1) as f32) / num_opts as f32;
    let slot_rect = |i: usize| {
        Rect::from_min_size(
            pos2(
                track_rect.left() + (slot_width + SEG_GAP) * i as f32,
                track_rect.top(),
            ),
            vec2(slot_width, SEG_H),
        )
    };
    let slot_at = |pos: Pos2| -> usize {
        let rel_x = (pos.x - track_rect.left()).clamp(0.0, track_rect.width());
        ((rel_x / (slot_width + SEG_GAP)).floor() as usize).min(num_opts - 1)
    };

    if response.clicked() || response.dragged() {
        if let Some(mouse_pos) = response.interact_pointer_pos() {
            let target_idx = slot_at(mouse_pos);
            if target_idx != current_idx || response.clicked() {
                on_select(&mut response, target_idx);
            }
        }
    }

    if ui.is_rect_visible(rect) {
        let is_enabled = ui.is_enabled();
        let painter = fade_cleared_painter(ui);
        let caption_color = if is_enabled {
            theme::TEXT_SECONDARY
        } else {
            theme::TEXT_MUTED
        };
        let font = FontId::new(FONT_SIZE, FontFamily::Proportional);

        match caption {
            Caption::Above => {
                painter.text(
                    pos2(track_rect.center().x, rect.top() + 1.0),
                    Align2::CENTER_TOP,
                    label,
                    font.clone(),
                    caption_color,
                );
            }
            Caption::Left => {
                painter.text(
                    pos2(rect.left(), track_rect.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    font.clone(),
                    caption_color,
                );
            }
        }

        let hovered_idx = if is_enabled {
            response.hover_pos().map(slot_at)
        } else {
            None
        };
        for (i, opt) in options.iter().enumerate() {
            let seg = slot_rect(i);
            let (fill, text) = segment_colors(i == current_idx, hovered_idx == Some(i), is_enabled);
            painter.rect_filled(seg, segment_corners(i, num_opts, false), fill);
            painter.text(
                seg.center(),
                Align2::CENTER_CENTER,
                *opt,
                font.clone(),
                text,
            );
        }
    }

    response
}

fn draw_wave_glyph(painter: &Painter, idx: usize, c: Pos2, color: Color32) {
    let stroke = Stroke::new(1.2_f32, color);
    let seg = |a: Pos2, b: Pos2| painter.line_segment([a, b], stroke);
    match idx {
        0 => {
            // Saw
            seg(pos2(c.x - 4.0, c.y + 3.0), pos2(c.x + 3.0, c.y - 3.0));
            seg(pos2(c.x + 3.0, c.y - 3.0), pos2(c.x + 3.0, c.y + 3.0));
        }
        1 => {
            // Square
            seg(pos2(c.x - 4.0, c.y + 3.0), pos2(c.x - 4.0, c.y - 3.0));
            seg(pos2(c.x - 4.0, c.y - 3.0), pos2(c.x, c.y - 3.0));
            seg(pos2(c.x, c.y - 3.0), pos2(c.x, c.y + 3.0));
            seg(pos2(c.x, c.y + 3.0), pos2(c.x + 4.0, c.y + 3.0));
        }
        2 => {
            // Triangle
            seg(pos2(c.x - 4.0, c.y + 3.0), pos2(c.x, c.y - 3.0));
            seg(pos2(c.x, c.y - 3.0), pos2(c.x + 4.0, c.y + 3.0));
        }
        3 => {
            // Sine
            let mut prev = pos2(c.x - 4.0, c.y);
            for step in 1..=8 {
                let t = step as f32 / 8.0;
                let cur = pos2(
                    c.x - 4.0 + t * 8.0,
                    c.y - (t * std::f32::consts::TAU).sin() * 3.0,
                );
                seg(prev, cur);
                prev = cur;
            }
        }
        _ => {}
    }
}

pub struct SlideSwitch<'a, T: nih_plug::prelude::Enum + PartialEq + 'static> {
    param: &'a EnumParam<T>,
    setter: &'a ParamSetter<'a>,
    label: &'a str,
    options: &'a [&'a str],
    is_vertical: bool,
    fixed_width: Option<f32>,
    caption: Caption,
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
            is_vertical: false,
            fixed_width: None,
            caption: Caption::Above,
        }
    }

    /// Stretch the horizontal track to `width` instead of hugging the option
    /// labels — used when a switch must fill a layout column so no dead band
    /// remains beside it.
    pub fn with_width(mut self, width: f32) -> Self {
        self.fixed_width = Some(width);
        self
    }

    /// Draw the caption left of the segments instead of above them.
    pub fn caption_left(mut self) -> Self {
        self.caption = Caption::Left;
        self
    }

    /// Stack the options vertically, each with a waveform glyph.
    pub fn vertical(mut self) -> Self {
        self.is_vertical = true;
        self
    }
}

impl<'a, T: nih_plug::prelude::Enum + PartialEq + 'static> Widget for SlideSwitch<'a, T> {
    fn ui(self, ui: &mut Ui) -> Response {
        let num_opts = self.options.len().max(1);
        let current_idx = self.param.value().to_index();
        let set_index = |setter: &ParamSetter, param: &EnumParam<T>, idx: usize| {
            setter.begin_set_parameter(param);
            setter.set_parameter_normalized(param, idx as f32 / (num_opts - 1).max(1) as f32);
            setter.end_set_parameter(param);
        };

        if self.is_vertical {
            const SLOT_H: f32 = SEG_H;
            let max_text_width = self
                .options
                .iter()
                .map(|opt| text_width(ui, opt))
                .fold(0.0f32, f32::max);
            let width = (20.0 + max_text_width + 16.0).max(52.0);
            let track_height = SLOT_H * num_opts as f32 + SEG_GAP * (num_opts - 1) as f32;
            let (rect, mut response) = ui.allocate_exact_size(
                vec2(width, CAPTION_H + track_height),
                Sense::click_and_drag(),
            );
            let track_rect = Rect::from_min_size(
                pos2(rect.left(), rect.top() + CAPTION_H),
                vec2(width, track_height),
            );
            let slot_at = |pos: Pos2| -> usize {
                let rel_y = (pos.y - track_rect.top()).clamp(0.0, track_rect.height() - 0.001);
                ((rel_y / (SLOT_H + SEG_GAP)).floor() as usize).min(num_opts - 1)
            };

            if response.clicked() || response.dragged() {
                if let Some(mouse_pos) = response.interact_pointer_pos() {
                    let target_idx = slot_at(mouse_pos);
                    if target_idx != current_idx || response.clicked() {
                        set_index(self.setter, self.param, target_idx);
                        response.mark_changed();
                    }
                }
            }

            if ui.is_rect_visible(rect) {
                let is_enabled = ui.is_enabled();
                let painter = fade_cleared_painter(ui);
                let font = FontId::new(FONT_SIZE, FontFamily::Proportional);
                painter.text(
                    pos2(rect.left(), rect.top() + 1.0),
                    Align2::LEFT_TOP,
                    self.label,
                    font.clone(),
                    if is_enabled {
                        theme::TEXT_SECONDARY
                    } else {
                        theme::TEXT_MUTED
                    },
                );

                let hovered_idx = if is_enabled {
                    response.hover_pos().map(slot_at)
                } else {
                    None
                };
                for (i, opt) in self.options.iter().enumerate() {
                    let seg = Rect::from_min_size(
                        pos2(
                            track_rect.left(),
                            track_rect.top() + (SLOT_H + SEG_GAP) * i as f32,
                        ),
                        vec2(width, SLOT_H),
                    );
                    let (fill, text) =
                        segment_colors(i == current_idx, hovered_idx == Some(i), is_enabled);
                    painter.rect_filled(seg, segment_corners(i, num_opts, true), fill);
                    draw_wave_glyph(&painter, i, pos2(seg.left() + 13.0, seg.center().y), text);
                    painter.text(
                        pos2(seg.left() + 25.0, seg.center().y),
                        Align2::LEFT_CENTER,
                        *opt,
                        font.clone(),
                        text,
                    );
                }
            }

            return response;
        }

        render_hsegment_row(
            ui,
            self.label,
            self.caption,
            self.options,
            current_idx,
            self.fixed_width,
            &mut |response, target_idx| {
                set_index(self.setter, self.param, target_idx);
                response.mark_changed();
            },
        )
    }
}

pub struct BoolSwitch<'a> {
    param: &'a nih_plug::prelude::BoolParam,
    setter: &'a ParamSetter<'a>,
    label: &'a str,
    options: [&'a str; 2],
    fixed_width: Option<f32>,
    caption: Caption,
}

impl<'a> BoolSwitch<'a> {
    pub fn new(
        param: &'a nih_plug::prelude::BoolParam,
        setter: &'a ParamSetter<'a>,
        label: &'a str,
        options: [&'a str; 2],
    ) -> Self {
        Self {
            param,
            setter,
            label,
            options,
            fixed_width: None,
            caption: Caption::Above,
        }
    }

    pub fn with_width(mut self, width: f32) -> Self {
        self.fixed_width = Some(width);
        self
    }

    /// Draw the caption left of the segments instead of above them.
    pub fn caption_left(mut self) -> Self {
        self.caption = Caption::Left;
        self
    }
}

impl<'a> Widget for BoolSwitch<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let current_idx = usize::from(self.param.value());
        render_hsegment_row(
            ui,
            self.label,
            self.caption,
            &self.options,
            current_idx,
            self.fixed_width,
            &mut |response, target_idx| {
                let new_val = target_idx == 1;
                self.setter.begin_set_parameter(self.param);
                self.setter.set_parameter(self.param, new_val);
                self.setter.end_set_parameter(self.param);
                response.mark_changed();
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A disabled control must never claim the accent: the selected segment
    /// keeps its position but drops to a neutral fill.
    #[test]
    fn disabled_selection_is_neutral() {
        let (fill, _) = segment_colors(true, false, false);
        assert_ne!(fill, theme::ACCENT_ORANGE);
        let (fill, _) = segment_colors(true, false, true);
        assert_eq!(fill, theme::ACCENT_ORANGE);
    }

    /// Every segment of a disabled bar stays a visible tile on both planes a
    /// bar sits on: at least the quietest plane step the design uses (header
    /// strip on body). SURFACE_PRESSED on the header strip was 1.01:1, and
    /// the bar fell apart into loose words (round-22 #10).
    #[test]
    fn disabled_segments_show_on_their_planes() {
        let floor = theme::contrast_ratio(theme::PANEL_HEADER_BG, theme::PANEL_BG);
        for selected in [true, false] {
            let (fill, _) = segment_colors(selected, false, false);
            for plane in [theme::PANEL_HEADER_BG, theme::PANEL_BG] {
                let ratio = theme::contrast_ratio(fill, plane);
                assert!(
                    ratio >= floor,
                    "selected={selected} on {plane:?}: {ratio:.2} < {floor:.2}"
                );
            }
        }
    }
}
