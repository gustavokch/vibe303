use crate::gui::theme;
use crate::params::SequencerPattern;
use nih_plug_egui::egui::{
    pos2, vec2, Align, Align2, Area, FontId, Frame, Id, Key, Margin, Order, Rect, RichText,
    ScrollArea, Sense, Stroke,
};

pub const TOTAL_PIANO_ROLL_ROWS: usize = 61; // C1 (row 60) through C6 (row 0)
pub const POPUP_WIDTH: f32 = 145.0;
pub const POPUP_HEIGHT: f32 = 255.0;
pub const ROW_HEIGHT: f32 = 19.0;
/// Frame margin inside the layer edge.
const INNER_MARGIN: i8 = 4;
/// Space between the popup's outer edge and its contents, on each side.
const INSET: f32 = INNER_MARGIN as f32 + theme::LAYER_EDGE.width;
pub const PIANO_ROLL_LABELS: [&str; TOTAL_PIANO_ROLL_ROWS] = [
    "C6", "B5", "A#5", "A5", "G#5", "G5", "F#5", "F5", "E5", "D#5", "D5",
    "C#5", // Rows 0..=11
    "C5", "B4", "A#4", "A4", "G#4", "G4", "F#4", "F4", "E4", "D#4", "D4",
    "C#4", // Rows 12..=23
    "C4", "B3", "A#3", "A3", "G#3", "G3", "F#3", "F3", "E3", "D#3", "D3",
    "C#3", // Rows 24..=35
    "C3", "B2", "A#2", "A2", "G#2", "G2", "F#2", "F2", "E2", "D#2", "D2",
    "C#2", // Rows 36..=47
    "C2", "B1", "A#1", "A1", "G#1", "G1", "F#1", "F1", "E1", "D#1", "D1",
    "C#1", // Rows 48..=59
    "C1",  // Row 60
];

#[inline]
pub fn row_index_to_label(row_idx: usize) -> &'static str {
    if row_idx < TOTAL_PIANO_ROLL_ROWS {
        PIANO_ROLL_LABELS[row_idx]
    } else {
        "C1"
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OpenPianoRollState {
    pub step_idx: usize,
    pub anchor_rect: Rect,
}

pub fn is_semitone_black_key(semitone: i8) -> bool {
    matches!(semitone, 1 | 3 | 6 | 8 | 10)
}

pub fn note_name_for_semitone(semitone: i8) -> &'static str {
    match semitone.clamp(0, 12) {
        0 => "C",
        1 => "C#",
        2 => "D",
        3 => "D#",
        4 => "E",
        5 => "F",
        6 => "F#",
        7 => "G",
        8 => "G#",
        9 => "A",
        10 => "A#",
        11 => "B",
        12 => "C",
        _ => "C",
    }
}

/// Maps `(semitone: 0..=12, octave: -2..=2)` to vertical row index `0..=60`
/// Row 0 is highest (C6 / C in octave 2, semitone 12), Row 60 is lowest (C1 / C in octave -2, semitone 0).
pub fn pitch_to_row_index(semitone: i8, octave: i8) -> usize {
    let pitch_offset = (octave as i32) * 12 + (semitone as i32);
    let clamped_offset = pitch_offset.clamp(-24, 36);
    (36 - clamped_offset) as usize
}

/// Maps vertical row index `0..=60` back to `(semitone: 0..=12, octave: -2..=2)`.
pub fn row_index_to_pitch(row_idx: usize) -> (i8, i8) {
    let clamped_row = (row_idx.min(TOTAL_PIANO_ROLL_ROWS - 1)) as i32;
    let pitch_offset = 36 - clamped_row;
    if pitch_offset >= 36 {
        (12, 2)
    } else {
        let octave = pitch_offset.div_euclid(12) as i8;
        let semitone = pitch_offset.rem_euclid(12) as i8;
        (semitone, octave)
    }
}

pub fn get_open_state(ctx: &nih_plug_egui::egui::Context) -> Option<OpenPianoRollState> {
    ctx.data(|d| d.get_temp::<OpenPianoRollState>(Id::new("sequencer_piano_roll_state")))
}

pub fn set_open_state(ctx: &nih_plug_egui::egui::Context, state: Option<OpenPianoRollState>) {
    ctx.data_mut(|d| {
        if let Some(s) = state {
            d.insert_temp(Id::new("sequencer_piano_roll_state"), s);
        } else {
            d.remove::<OpenPianoRollState>(Id::new("sequencer_piano_roll_state"));
        }
    });
}

pub fn toggle_open_state(ctx: &nih_plug_egui::egui::Context, step_idx: usize, anchor_rect: Rect) {
    let current = get_open_state(ctx);
    if let Some(curr) = current {
        if curr.step_idx == step_idx {
            set_open_state(ctx, None);
            return;
        }
    }
    set_open_state(
        ctx,
        Some(OpenPianoRollState {
            step_idx,
            anchor_rect,
        }),
    );
}

/// Side of the close action, and of the cross painted in it.
const CLOSE_SIZE: f32 = 16.0;
const CROSS_SIZE: f32 = 6.0;

/// The close action: a raised tile with a painted cross. The font has no "✕"
/// glyph, so a text label painted an empty box (round-22 #1).
fn close_button(ui: &mut nih_plug_egui::egui::Ui) -> nih_plug_egui::egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(CLOSE_SIZE, CLOSE_SIZE), Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let fill = if response.hovered() {
            theme::SURFACE_HOVER
        } else {
            theme::SURFACE_ELEVATED
        };
        p.rect_filled(rect, theme::radius(), fill);
        let cross = Rect::from_center_size(rect.center(), vec2(CROSS_SIZE, CROSS_SIZE));
        let stroke = Stroke::new(1.5_f32, theme::TEXT_PRIMARY);
        p.line_segment([cross.left_top(), cross.right_bottom()], stroke);
        p.line_segment([cross.right_top(), cross.left_bottom()], stroke);
    }
    response.on_hover_text("Close (Esc)")
}

pub fn render_piano_roll_popup(
    ctx: &nih_plug_egui::egui::Context,
    pattern: &mut SequencerPattern,
) -> bool {
    let open_state = match get_open_state(ctx) {
        Some(s) if s.step_idx < pattern.steps.len() => s,
        _ => return false,
    };

    let step = &mut pattern.steps[open_state.step_idx];
    let mut changed = false;
    let mut should_close = false;

    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        set_open_state(ctx, None);
        return false;
    }

    let screen_rect = ctx.screen_rect();

    // Position popup anchored above the step button (or below if near the top)
    let anchor = open_state.anchor_rect;
    let mut target_x = anchor.center().x - (POPUP_WIDTH / 2.0);
    target_x = target_x.clamp(8.0, screen_rect.width() - POPUP_WIDTH - 8.0);

    let target_y = if anchor.top() >= POPUP_HEIGHT + 10.0 {
        anchor.top() - POPUP_HEIGHT - 6.0
    } else {
        (anchor.bottom() + 6.0).min(screen_rect.height() - POPUP_HEIGHT - 8.0)
    };

    let popup_rect = Rect::from_min_size(pos2(target_x, target_y), vec2(POPUP_WIDTH, POPUP_HEIGHT));

    // Handle clicking outside to dismiss
    if ctx.input(|i| i.pointer.any_pressed()) {
        if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
            if !popup_rect.contains(pos) && !anchor.contains(pos) {
                set_open_state(ctx, None);
                return false;
            }
        }
    }

    let active_row = pitch_to_row_index(step.semitone, step.octave);
    let scrolled_init_id = Id::new("piano_roll_scrolled_to_step").with(open_state.step_idx);
    let is_scrolled_init = ctx.data(|d| d.get_temp::<bool>(scrolled_init_id).unwrap_or(false));

    Area::new(Id::new("sequencer_piano_roll_popup_area"))
        .order(Order::Foreground)
        .fixed_pos(pos2(target_x, target_y))
        .show(ctx, |ui| {
            Frame::new()
                .fill(theme::PANEL_BG)
                .stroke(theme::LAYER_EDGE)
                .corner_radius(theme::radius())
                .inner_margin(Margin::same(INNER_MARGIN))
                .show(ui, |ui| {
                    ui.set_width(POPUP_WIDTH - 2.0 * INSET);
                    ui.set_height(POPUP_HEIGHT - 2.0 * INSET);

                    // Header bar: the title in words (proportional), the close
                    // action flush right.
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("STEP {:02} NOTE", open_state.step_idx + 1))
                                .font(FontId::proportional(9.0))
                                .color(theme::TEXT_PRIMARY),
                        );

                        ui.with_layout(
                            nih_plug_egui::egui::Layout::right_to_left(Align::Center),
                            |ui| {
                                if close_button(ui).clicked() {
                                    should_close = true;
                                }
                            },
                        );
                    });

                    ui.add_space(2.0);
                    ui.separator();
                    ui.add_space(2.0);

                    // Scrollable piano roll, below the 24 pt title row and separator.
                    ScrollArea::vertical()
                        .max_height(POPUP_HEIGHT - 2.0 * INSET - 24.0)
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = vec2(0.0, 1.0);

                            for row_idx in 0..TOTAL_PIANO_ROLL_ROWS {
                                let (semitone, octave) = row_index_to_pitch(row_idx);
                                let is_active = row_idx == active_row;
                                let is_black = is_semitone_black_key(semitone);
                                let note_name = note_name_for_semitone(semitone);
                                let full_note_str = row_index_to_label(row_idx);

                                let row_width = ui.available_width();
                                let (row_rect, row_resp) = ui.allocate_exact_size(
                                    vec2(row_width, ROW_HEIGHT),
                                    Sense::click(),
                                );

                                if !is_scrolled_init && is_active {
                                    row_resp.scroll_to_me(Some(Align::Center));
                                    ctx.data_mut(|d| d.insert_temp(scrolled_init_id, true));
                                }

                                if row_resp.clicked() {
                                    step.semitone = semitone;
                                    step.octave = octave;
                                    changed = true;
                                    should_close = true;
                                }

                                if ui.is_rect_visible(row_rect) {
                                    let p = ui.painter();
                                    let is_hovered = row_resp.hovered();

                                    // Key Cap styling (left portion)
                                    let key_cap_width = 40.0;
                                    let key_rect = Rect::from_min_size(
                                        row_rect.min,
                                        vec2(key_cap_width, ROW_HEIGHT),
                                    );
                                    // Background fill for the full row
                                    let row_bg = if is_active {
                                        theme::SURFACE_ELEVATED
                                    } else if is_hovered {
                                        theme::SURFACE_PRESSED
                                    } else {
                                        theme::PANEL_BG
                                    };
                                    p.rect_filled(row_rect, theme::radius(), row_bg);

                                    // Piano key graphic:
                                    // White keys: GATE_NOTE_FILL with TEXT_ON_ACCENT labels (hover: TEXT_PRIMARY)
                                    // Black keys: WELL_BG with TEXT_SECONDARY labels (hover: SURFACE_PRESSED)
                                    // Selected key: ACCENT_ORANGE with TEXT_ON_ACCENT
                                    let (key_fill, key_text_color) = if is_active {
                                        (theme::ACCENT_ORANGE, theme::TEXT_ON_ACCENT)
                                    } else if is_black {
                                        if is_hovered {
                                            (theme::SURFACE_PRESSED, theme::TEXT_SECONDARY)
                                        } else {
                                            (theme::WELL_BG, theme::TEXT_SECONDARY)
                                        }
                                    } else if is_hovered {
                                        (theme::TEXT_PRIMARY, theme::TEXT_ON_ACCENT)
                                    } else {
                                        (theme::GATE_NOTE_FILL, theme::TEXT_ON_ACCENT)
                                    };

                                    p.rect_filled(key_rect, theme::radius(), key_fill);
                                    // Note label on key
                                    p.text(
                                        pos2(key_rect.center().x, key_rect.center().y),
                                        Align2::CENTER_CENTER,
                                        note_name,
                                        FontId::monospace(8.5),
                                        key_text_color,
                                    );

                                    // Full pitch descriptor + octave indicator on right.
                                    // The orange key alone marks the active row; orange
                                    // text on the row highlight was 3.53:1 (round-22 #5).
                                    let label_pos = pos2(row_rect.max.x - 6.0, row_rect.center().y);
                                    let full_label_color = if is_active || is_hovered {
                                        theme::TEXT_PRIMARY
                                    } else {
                                        theme::TEXT_SECONDARY
                                    };

                                    p.text(
                                        label_pos,
                                        Align2::RIGHT_CENTER,
                                        full_note_str,
                                        FontId::monospace(9.0),
                                        full_label_color,
                                    );
                                    // Octave dividing line on 'C' notes
                                    if semitone == 0 {
                                        p.line_segment(
                                            [
                                                pos2(row_rect.left(), row_rect.bottom()),
                                                pos2(row_rect.right(), row_rect.bottom()),
                                            ],
                                            Stroke::new(1.0_f32, theme::WELL_BG),
                                        );
                                    }
                                }
                            }
                        });
                });
        });

    if should_close {
        set_open_state(ctx, None);
    }

    changed
}
