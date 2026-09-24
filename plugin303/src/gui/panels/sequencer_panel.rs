use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    epaint::Shape, style::WidgetVisuals, vec2, AboveOrBelow, Align, Button, ComboBox, CornerRadius,
    DragValue, FontId, Frame, Layout, Margin, Rect, Sense, Stroke, Ui,
};
use std::sync::atomic::{AtomicU8, Ordering};

use crate::gui::theme;
use crate::gui::widgets::piano_roll_popup::render_piano_roll_popup;
use crate::gui::widgets::render_step_strip;
use crate::gui::widgets::step_button::STRIP_HEIGHT;
use crate::params::{Plugin303Params, SequencerStep, StepGate};

/// Custom ComboBox icon painter that always paints a downward-pointing triangle (▼).
/// These two bottom-row dropdowns sit low enough in the window that egui flips the glyph
/// when the popup opens upward; this painter keeps the glyph downward-pointing to match
/// the app-wide dropdown convention.
fn paint_combobox_down_arrow(
    ui: &Ui,
    rect: Rect,
    visuals: &WidgetVisuals,
    _is_open: bool,
    _above_or_below: AboveOrBelow,
) {
    let rect = Rect::from_center_size(
        rect.center(),
        vec2(rect.width() * 0.7, rect.height() * 0.45),
    );
    ui.painter().add(Shape::convex_polygon(
        vec![rect.left_top(), rect.right_top(), rect.center_bottom()],
        visuals.fg_stroke.color,
        Stroke::NONE,
    ));
}

/// Width reserved for the transport/pattern column right of the step grid.
const TRANSPORT_COLUMN_W: f32 = 280.0;
/// Horizontal gap between adjacent step cells (and grid rows vertically).
const GRID_GAP: f32 = 4.0;
/// Total vertical height of the 2-row step grid (2 * STRIP_HEIGHT + GRID_GAP = 268.0).
const GRID_TOTAL_H: f32 = 2.0 * STRIP_HEIGHT + GRID_GAP;
/// Approximate painted height of the transport column title row, subtracted
/// before the remaining column height is split across the transport rows
/// (round-13 issue 4).
const TRANSPORT_TITLE_H: f32 = 13.0;
/// Number of transport rows sharing the column's vertical extent.
const TRANSPORT_ROW_COUNT: f32 = 5.0;
/// Vertical gap between the transport title and each transport row. Used by
/// both `item_spacing` and `transport_row_height` so the stretch math always
/// matches the painted spacing (PR#61 review).
const TRANSPORT_ROW_GAP: f32 = 6.0;
/// Minimum transport row height; below this the rows would look cramped.
const TRANSPORT_ROW_MIN_H: f32 = 24.0;
/// Half of the transport column's inner width: every transport row is laid out
/// on a two-slot grid (slot, gap, slot) so all rows share left/right edges and
/// reach the panel's right inner margin (round-11 items 4 and 21).
const TRANSPORT_SLOT_W: f32 = 138.0;
/// Vertical chrome subtracted from the pre-frame available height so the
/// frame bottom lands on the bottom outer margin (20 physical px, matching
/// the other three sides): just a 2-logical breathing gap above the chassis
/// content edge. The frame's own inner margins live inside its height, so
/// they do not subtract here — the old 21.0 (margins 16 + stroke 1 +
/// trailing spacing 4) pushed the frame bottom 42 physical px above the
/// content edge, which read as an oversized bottom margin once the floor
/// stopped dominating (round-19 issue 8, fixed in the review follow-up).
///
/// The fix funds itself from the panels above: the souped drawer repack
/// (SlideSwitch height 24, separator spacing 2, knob row 63) freed ~13
/// logical px, so at 1366x992 `avail_h - FRAME_VERT_CHROME_PX` (~284.5)
/// now exceeds the grid floor (`GRID_TOTAL_H + 16` = 284) and the frame
/// bottom border renders inside the window.
const FRAME_VERT_CHROME_PX: f32 = 2.0;
/// `add_space(8)` + separator widget (~7 with its margins) + `add_space(8)`
/// between grid and transport column. Measured on-canvas: the run costs 23.1 px
/// with item_spacing.x = 0 (round-11 item 4 fix).
const SEPARATOR_CHROME_W: f32 = 24.0;
/// Smallest usable step cell; below this the grid overflows instead of scrolling.
const MIN_CELL_W: f32 = 72.0;

/// Hit-rect width for the JITTER toggle: 15 px text offset + ~27 px for
/// "JITTER" at 8 pt + 4 px margin, so the painted label never overflows onto
/// the CLEAR button (PR#53 review).
const JIT_HIT_W: f32 = 52.0;

/// Panel-gap hierarchy (round-17 issue 2). The `panels` probe reports a 0 px
/// gap above the transport row because the transport controls are not a
/// separate framed panel: they are the right column of the same sequencer
/// `Frame` as the step grid (GRID_GAP = 4 logical px separates grid rows and
/// cells inside it). Top-level separation from the generator panel above is
/// the uniform `PANEL_GAP` spacer at the top of
/// `render_sequencer_and_transport`; do not add another spacer inside this
/// frame.
///
/// Compute the adaptive minimum height for the sequencer frame.
fn frame_min_height(avail_h: f32) -> f32 {
    (avail_h - FRAME_VERT_CHROME_PX).max(GRID_TOTAL_H + 16.0)
}

/// Stretched height of each transport row: the five rows, the title and the
/// gaps between them fill the column exactly, leaving no dead band below the
/// last row (round-13 issue 4).
fn transport_row_height(avail_h: f32) -> f32 {
    ((avail_h - TRANSPORT_TITLE_H - TRANSPORT_ROW_COUNT * TRANSPORT_ROW_GAP) / TRANSPORT_ROW_COUNT)
        .max(TRANSPORT_ROW_MIN_H)
}

/// Per-slot setup shared by the transport column's `allocate_ui_with_layout`
/// slots: the standard 4-px internal gutter and one height for every control
/// in the slot via `interact_size.y = row_h`, so a slot cannot size itself off
/// egui's default `interact_size` and measure against its row (round-14
/// issue 5). Row C's BPM/LEN steppers deliberately keep a tighter 3.0 gutter
/// and override `item_spacing` after calling this (round-11 item 24).
fn begin_transport_slot(ui: &mut Ui, row_h: f32) {
    ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
    ui.spacing_mut().interact_size.y = row_h;
}

/// Snap a dragged BPM value to a whole number so the `{:.0}` display always
/// matches the stored value (PR#53 review).
fn snap_tempo(v: f32) -> f32 {
    v.round()
}

/// Step cell width for a given inner available width.
/// Returns `(cell_w, clamped)` — `clamped` is true when the window is too
/// narrow and cells will overflow (surfaced by a debug assertion at call site).
pub(crate) fn grid_cell_width(avail_w: f32) -> (f32, bool) {
    let grid_avail_w = avail_w - TRANSPORT_COLUMN_W - SEPARATOR_CHROME_W;
    let unclamped = (grid_avail_w - 7.0 * GRID_GAP) / 8.0;
    if unclamped < MIN_CELL_W {
        (MIN_CELL_W, true)
    } else {
        (unclamped, false)
    }
}

pub fn render_sequencer_and_transport(
    ui: &mut Ui,
    params: &Plugin303Params,
    gen_params: &mut crate::dsp::generator::GeneratorParams,
    setter: &ParamSetter,
    current_step_atomic: &AtomicU8,
) {
    let active_step = current_step_atomic.load(Ordering::Relaxed);

    super::generator_panel::render_generator_panel(ui, params, gen_params, setter);
    // Same uniform gutter as the other five panel gaps (round-13 issue 11);
    // the parent stack zeroes item_spacing.y, so this spacer is the whole gap.
    ui.add_space(crate::gui::PANEL_GAP);

    let frame_avail_h = ui.available_height();

    // The frame gets an explicit height: a bare `Frame::show(ui, ..)` takes
    // the whole remaining available rect and, because the transport column
    // stretches to fill it, the painted frame consistently overshoots the
    // content bottom by ~8.5 logical px and its bottom stroke lands outside
    // the captured window (round-19 issue 8). Allocating the frame at
    // `frame_min_height` pins the bottom border to a deterministic position.
    let frame_h = frame_min_height(frame_avail_h);
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), frame_h),
        Layout::top_down(Align::LEFT),
        |ui| {
            Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin {
            left: 8,
            right: 8,
            top: 4,
            bottom: 12,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // Grid geometry: 2 rows x 8 columns; transport column on the right
            let (cell_w, clamped) = grid_cell_width(ui.available_width());
            debug_assert!(
                !clamped,
                "sequencer grid overflows available width; cells clamp to {MIN_CELL_W}px and squeeze the transport column"
            );
            let cell_h = STRIP_HEIGHT;

            ui.horizontal(|ui| {
                // item_spacing.x = 0: the grid→separator→transport run is
                // spaced with explicit add_space(8) calls, so the real chrome
                // matches SEPARATOR_CHROME_W (18) and the transport column
                // gets its full reserved 280 px (round-11 item 4; the default
                // 8 px spacing made the separator run ~49 px and squeezed the
                // 280 px transport rows past the panel edge).
                ui.spacing_mut().item_spacing.x = 0.0;
                // 1. LEFT / MAIN: step pitch key columns in a 2x8 grid (steps 01-08 top, 09-16 bottom)
                ui.vertical(|ui| {
                    if let Ok(mut pattern_guard) = params.pattern.write() {
                        let is_running = params.seq_run.value();
                        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);

                        for row in 0..2 {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = vec2(4.0, 0.0);

                                for col in 0..8 {
                                    let step_idx = row * 8 + col;
                                    let is_current = active_step as usize == step_idx;
                                    let step = &mut pattern_guard.steps[step_idx];

                                    render_step_strip(ui, step_idx, step, is_current, is_running, cell_w, cell_h);
                                }
                            });
                        }

                        render_piano_roll_popup(ui.ctx(), &mut pattern_guard);
                    }
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // 2. RIGHT COLUMN: Wide Transport & Pattern Controls
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing = vec2(3.0, TRANSPORT_ROW_GAP);
                    ui.spacing_mut().button_padding = vec2(3.0, 1.0);

                    // Stretch the five transport rows across the column's full
                    // height so no dead band remains below the last row
                    // (round-13 issue 4). The frame's min height reaches the
                    // window bottom, so available_height is the whole column.
                    // Five rows plus five gaps (title→A … C→D) share it.
                    let row_h = transport_row_height(ui.available_height());

                    ui.label(
                        nih_plug_egui::egui::RichText::new("TRANSPORT & PATTERN")
                            .font(FontId::proportional(9.0))
                            .strong()
                            // Shared accent title colour (round-11 item 8).
                            .color(theme::ACCENT_ORANGE),
                    );

                    // Row A: Run/Sync Buttons. Boolean state uses one encoding
                    // across the transport panel: engaged = orange fill with
                    // dark label, not engaged = dark well with muted label
                    // (round-11 item 13).
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                        let run_val = params.seq_run.value();
                        let run_btn = Button::new(
                            nih_plug_egui::egui::RichText::new(if run_val {
                                "STOP"
                            } else {
                                "RUN"
                            })
                            .font(FontId::proportional(9.0))
                            .strong()
                            .color(if run_val {
                                theme::TEXT_ON_ACCENT
                            } else {
                                theme::TEXT_SECONDARY
                            }),
                        )
                        .fill(if run_val {
                            theme::ACCENT_ORANGE
                        } else {
                            theme::WELL_BG
                        })
                        .stroke(Stroke::new(1.0_f32, if run_val { theme::ACCENT_ORANGE } else { theme::BORDER_DEFAULT }))
                        .min_size(vec2(TRANSPORT_SLOT_W, row_h))
                        .corner_radius(CornerRadius::same(2));

                        if ui.add(run_btn).clicked() {
                            setter.begin_set_parameter(&params.seq_run);
                            setter.set_parameter(&params.seq_run, !run_val);
                            setter.end_set_parameter(&params.seq_run);
                        }

                        let sync_val = params.seq_sync.value();
                        let sync_btn = Button::new(
                            nih_plug_egui::egui::RichText::new(if sync_val {
                                "SYNC: HOST"
                            } else {
                                "SYNC: INT"
                            })
                            .font(FontId::proportional(8.5))
                            .strong()
                            .color(if sync_val {
                                theme::TEXT_ON_ACCENT
                            } else {
                                theme::TEXT_SECONDARY
                            }),
                        )
                        .fill(if sync_val {
                            theme::ACCENT_ORANGE
                        } else {
                            theme::WELL_BG
                        })
                        .stroke(Stroke::new(1.0_f32, if sync_val { theme::ACCENT_ORANGE } else { theme::BORDER_DEFAULT }))
                        .min_size(vec2(TRANSPORT_SLOT_W, row_h))
                        .corner_radius(CornerRadius::same(2));

                        if ui.add(sync_btn).clicked() {
                            setter.begin_set_parameter(&params.seq_sync);
                            setter.set_parameter(&params.seq_sync, !sync_val);
                            setter.end_set_parameter(&params.seq_sync);
                        }
                    });

                    // Row: MIDI Out Toggle + Channel + Synth Bypass — same
                    // two-slot grid; SYNTH follows the shared boolean encoding
                    // (muted = not engaged, so it loses the ruby accent).
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 0.0);

                        ui.allocate_ui_with_layout(
                            vec2(TRANSPORT_SLOT_W, row_h),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                begin_transport_slot(ui, row_h);

                                // MIDI OUT button
                                let midi_val = params.midi_out.value();
                                let midi_btn = Button::new(
                                    nih_plug_egui::egui::RichText::new(if midi_val { "MIDI: ON" } else { "MIDI: OFF" })
                                        .font(FontId::proportional(8.5))
                                        .strong()
                                        .color(if midi_val { theme::TEXT_ON_ACCENT } else { theme::TEXT_SECONDARY }),
                                )
                                .fill(if midi_val { theme::ACCENT_ORANGE } else { theme::WELL_BG })
                                .stroke(Stroke::new(1.0_f32, if midi_val { theme::ACCENT_ORANGE } else { theme::BORDER_DEFAULT }))
                                .min_size(vec2(70.0, row_h))
                                .corner_radius(CornerRadius::same(2));

                                if ui.add(midi_btn).on_hover_text("Toggle MIDI Output from 16-step sequencer").clicked() {
                                    setter.begin_set_parameter(&params.midi_out);
                                    setter.set_parameter(&params.midi_out, !midi_val);
                                    setter.end_set_parameter(&params.midi_out);
                                }

                                // MIDI Channel selector. The field stretches to
                                // the slot's right edge so the row leaves no
                                // dead run on the right (round-13 issue 4).
                                ui.label(
                                    nih_plug_egui::egui::RichText::new("CH:")
                                        .font(FontId::proportional(8.0))
                                        .color(theme::TEXT_MUTED),
                                );
                                let mut ch_val = params.midi_out_channel.value();
                                if ui
                                    .add_sized(
                                        vec2(ui.available_width(), row_h),
                                        DragValue::new(&mut ch_val)
                                            .range(1..=16)
                                            .custom_formatter(|n, _| format!("{:02}", n as i32)),
                                    )
                                    .on_hover_text("MIDI Output Channel (1-16)")
                                    .changed()
                                {
                                    setter.begin_set_parameter(&params.midi_out_channel);
                                    setter.set_parameter(&params.midi_out_channel, ch_val);
                                    setter.end_set_parameter(&params.midi_out_channel);
                                }
                            },
                        );

                        // SYNTH BYPASS button
                        let bypass_val = params.synth_bypass.value();
                        let bypass_btn = Button::new(
                            nih_plug_egui::egui::RichText::new(if bypass_val { "SYNTH: MUTE" } else { "SYNTH: ON" })
                                .font(FontId::proportional(8.5))
                                .strong()
                                .color(if bypass_val { theme::TEXT_SECONDARY } else { theme::TEXT_ON_ACCENT }),
                        )
                        .fill(if bypass_val { theme::WELL_BG } else { theme::ACCENT_ORANGE })
                        .stroke(Stroke::new(1.0_f32, if bypass_val { theme::BORDER_DEFAULT } else { theme::ACCENT_ORANGE }))
                        .min_size(vec2(TRANSPORT_SLOT_W, row_h))
                        .corner_radius(CornerRadius::same(2));

                        if ui.add(bypass_btn).on_hover_text("Bypass internal synth voice (outputs silence, 0% CPU)").clicked() {
                            setter.begin_set_parameter(&params.synth_bypass);
                            setter.set_parameter(&params.synth_bypass, !bypass_val);
                            setter.end_set_parameter(&params.synth_bypass);
                        }
                    });

                    // Row B: Cyclic Pattern Shift Controls (< SHIFT and > SHIFT)
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 0.0);

                        let shift_left_btn = Button::new(
                            nih_plug_egui::egui::RichText::new("< SHIFT")
                                .font(FontId::proportional(8.5))
                                .strong()
                                .color(theme::TEXT_PRIMARY),
                        )
                        .fill(theme::SURFACE_ELEVATED)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER_DEFAULT))
                        .min_size(vec2(TRANSPORT_SLOT_W, row_h))
                        .corner_radius(CornerRadius::same(2));

                        if ui.add(shift_left_btn).on_hover_text("Rotate pattern steps left (cyclic)").clicked() {
                            if let Ok(mut pattern_guard) = params.pattern.write() {
                                pattern_guard.steps.rotate_left(1);
                            }
                        }

                        let shift_right_btn = Button::new(
                            nih_plug_egui::egui::RichText::new("> SHIFT")
                                .font(FontId::proportional(8.5))
                                .strong()
                                .color(theme::TEXT_PRIMARY),
                        )
                        .fill(theme::SURFACE_ELEVATED)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER_DEFAULT))
                        .min_size(vec2(TRANSPORT_SLOT_W, row_h))
                        .corner_radius(CornerRadius::same(2));

                        if ui.add(shift_right_btn).on_hover_text("Rotate pattern steps right (cyclic)").clicked() {
                            if let Ok(mut pattern_guard) = params.pattern.write() {
                                pattern_guard.steps.rotate_right(1);
                            }
                        }
                    });

                    // Row C: BPM + Length on the two-slot grid. The -/+ steppers
                    // share the field's dark well treatment with explicit label
                    // color so neither glyph renders lighter than the other
                    // (round-11 item 24).
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                        ui.allocate_ui_with_layout(
                            vec2(TRANSPORT_SLOT_W, row_h),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                begin_transport_slot(ui, row_h);
                                // Tighter gutter between the BPM steppers
                                // (round-11 item 24).
                                ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
                                ui.label(
                                    nih_plug_egui::egui::RichText::new("BPM:")
                                        .font(FontId::proportional(8.0))
                                        .color(theme::TEXT_MUTED),
                                );
                                let mut tempo_val = params.seq_tempo.value();

                                let dn_btn = Button::new(
                                    nih_plug_egui::egui::RichText::new("-")
                                        .font(FontId::proportional(9.0))
                                        .strong()
                                        .color(theme::TEXT_PRIMARY),
                                )
                                .fill(theme::WELL_BG)
                                .stroke(Stroke::new(1.0_f32, theme::BORDER_DEFAULT))
                                .min_size(vec2(24.0, row_h))
                                .corner_radius(CornerRadius::same(2));
                                if ui.add(dn_btn).clicked() {
                                    setter.begin_set_parameter(&params.seq_tempo);
                                    setter.set_parameter(
                                        &params.seq_tempo,
                                        (tempo_val - 1.0).clamp(40.0, 300.0),
                                    );
                                    setter.end_set_parameter(&params.seq_tempo);
                                }

                                // The field fills the slot between the steppers
                                // (24 px + 3 px gap on the right) so the row
                                // reaches the shared right edge (round-13 issue 4).
                                let dv_w = (ui.available_width() - 27.0).max(24.0);
                                if ui
                                    .add_sized(
                                        vec2(dv_w, row_h),
                                        DragValue::new(&mut tempo_val)
                                            .range(40.0..=300.0)
                                            .speed(0.1)
                                            .custom_formatter(|n, _| format!("{:.0}", n)),
                                    )
                                    .changed()
                                {
                                    setter.begin_set_parameter(&params.seq_tempo);
                                    setter.set_parameter(&params.seq_tempo, snap_tempo(tempo_val));
                                    setter.end_set_parameter(&params.seq_tempo);
                                }

                                let up_btn = Button::new(
                                    nih_plug_egui::egui::RichText::new("+")
                                        .font(FontId::proportional(9.0))
                                        .strong()
                                        .color(theme::TEXT_PRIMARY),
                                )
                                .fill(theme::WELL_BG)
                                .stroke(Stroke::new(1.0_f32, theme::BORDER_DEFAULT))
                                .min_size(vec2(24.0, row_h))
                                .corner_radius(CornerRadius::same(2));
                                if ui.add(up_btn).clicked() {
                                    setter.begin_set_parameter(&params.seq_tempo);
                                    setter.set_parameter(
                                        &params.seq_tempo,
                                        (tempo_val + 1.0).clamp(40.0, 300.0),
                                    );
                                    setter.end_set_parameter(&params.seq_tempo);
                                }
                            },
                        );

                        ui.allocate_ui_with_layout(
                            vec2(TRANSPORT_SLOT_W, row_h),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                begin_transport_slot(ui, row_h);
                                ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
                                ui.label(
                                    nih_plug_egui::egui::RichText::new("LEN:")
                                        .font(FontId::proportional(8.0))
                                        .color(theme::TEXT_MUTED),
                                );
                                let mut len_val = params.seq_length.value();
                                // The field fills the slot's remaining width so
                                // the row reaches the shared right edge
                                // (round-13 issue 4).
                                if ui
                                    .add_sized(
                                        vec2(ui.available_width(), row_h),
                                        DragValue::new(&mut len_val)
                                            .range(1..=16)
                                            .custom_formatter(|n, _| format!("{:02}", n as i32)),
                                    )
                                    .changed()
                                {
                                    setter.begin_set_parameter(&params.seq_length);
                                    setter.set_parameter(&params.seq_length, len_val);
                                    setter.end_set_parameter(&params.seq_length);
                                }
                            },
                        );
                    });

                    // Row D: Timebase + Mode + Jitter + Clear. Shares the
                    // stretched row height with the other four rows so the
                    // column has no dead band below it (round-13 issue 4).
                    // Two slots on the shared grid keep the row's right edge on
                    // the column's right inner margin.
                    //
                    // Round-19 issue 2: built with the same `ui.horizontal` +
                    // two-slot construction as rows B and C, instead of its own
                    // `allocate_ui_with_layout` outer, so its slot edges land
                    // on the same grid. The two combos also take `row_h` now —
                    // they used to size themselves off `interact_size` and
                    // measured 75 px physical against CLEAR's 175 px in the
                    // same row.
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                        ui.spacing_mut().button_padding = vec2(4.0, 3.0);

                        ui.allocate_ui_with_layout(
                            vec2(TRANSPORT_SLOT_W, row_h),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                begin_transport_slot(ui, row_h);
                                let tb_val = params.seq_timebase.value();
                                    ComboBox::from_id_salt("seq_tb_combo")
                                        .icon(paint_combobox_down_arrow)
                                        // Caption class (8.5 pt) like every
                                        // button in this row; the default
                                        // button style rendered these glyphs
                                        // at h=19 next to h=11 captions
                                        // (round-14 issue 5).
                                        .selected_text(
                                            nih_plug_egui::egui::RichText::new(match tb_val {
                                                crate::params::Timebase::Straight => "16th",
                                                crate::params::Timebase::Triplet => "12th",
                                            })
                                            .font(FontId::proportional(8.5)),
                                        )
                                        .width(48.0)
                                        .show_ui(ui, |ui| {
                                            if ui
                                                .selectable_label(
                                                    tb_val == crate::params::Timebase::Straight,
                                                    nih_plug_egui::egui::RichText::new("16th")
                                                        .font(FontId::proportional(8.5)),
                                                )
                                                .clicked()
                                            {
                                                setter.begin_set_parameter(&params.seq_timebase);
                                                setter.set_parameter(
                                                    &params.seq_timebase,
                                                    crate::params::Timebase::Straight,
                                                );
                                                setter.end_set_parameter(&params.seq_timebase);
                                            }
                                            if ui
                                                .selectable_label(
                                                    tb_val == crate::params::Timebase::Triplet,
                                                    nih_plug_egui::egui::RichText::new("12th")
                                                        .font(FontId::proportional(8.5)),
                                                )
                                                .clicked()
                                            {
                                                setter.begin_set_parameter(&params.seq_timebase);
                                                setter.set_parameter(
                                                    &params.seq_timebase,
                                                    crate::params::Timebase::Triplet,
                                                );
                                                setter.end_set_parameter(&params.seq_timebase);
                                            }
                                        });

                                    let mode_val = params.seq_mode.value();
                                    ComboBox::from_id_salt("seq_mode_combo")
                                        .icon(paint_combobox_down_arrow)
                                        .selected_text(
                                            nih_plug_egui::egui::RichText::new(match mode_val {
                                                crate::params::SequencerMode::DirectGrid => {
                                                    "Direct"
                                                }
                                                crate::params::SequencerMode::DecoupledHardware => {
                                                    "Decoupled"
                                                }
                                            })
                                            .font(FontId::proportional(8.5)),
                                        )
                                        // 48 + 4 gap + 86 fills the 138-px slot
                                        // exactly, so Direct's right edge lands
                                        // on the column boundary rows 1-4 use
                                        // (round-19 issue 2).
                                        .width(86.0)
                                        .show_ui(ui, |ui| {
                                            if ui
                                                .selectable_label(
                                                    mode_val
                                                        == crate::params::SequencerMode::DirectGrid,
                                                    nih_plug_egui::egui::RichText::new("Direct")
                                                        .font(FontId::proportional(8.5)),
                                                )
                                                .clicked()
                                            {
                                                setter.begin_set_parameter(&params.seq_mode);
                                                setter.set_parameter(
                                                    &params.seq_mode,
                                                    crate::params::SequencerMode::DirectGrid,
                                                );
                                                setter.end_set_parameter(&params.seq_mode);
                                            }
                                            if ui
                                                .selectable_label(
                                                    mode_val
                                                        == crate::params::SequencerMode::DecoupledHardware,
                                                    nih_plug_egui::egui::RichText::new("Decoupled")
                                                        .font(FontId::proportional(8.5)),
                                                )
                                                .clicked()
                                            {
                                                setter.begin_set_parameter(&params.seq_mode);
                                                setter.set_parameter(
                                                    &params.seq_mode,
                                                    crate::params::SequencerMode::DecoupledHardware,
                                                );
                                                setter.end_set_parameter(&params.seq_mode);
                                            }
                                        });
                                },
                            );

                            ui.allocate_ui_with_layout(
                                vec2(TRANSPORT_SLOT_W, row_h),
                                Layout::left_to_right(Align::Center),
                                |ui| {
                                    begin_transport_slot(ui, row_h);

                                    let mut jitter_val = params.seq_jitter.value();
                                    let (jit_hit_rect, jit_resp) = ui.allocate_exact_size(
                                        vec2(JIT_HIT_W, row_h),
                                        Sense::click(),
                                    );
                                    let jit_resp = jit_resp
                                        .on_hover_text("Clock jitter / micro-timing humanization");
                                    if jit_resp.clicked() {
                                        jitter_val = !jitter_val;
                                        setter.begin_set_parameter(&params.seq_jitter);
                                        setter.set_parameter(&params.seq_jitter, jitter_val);
                                        setter.end_set_parameter(&params.seq_jitter);
                                    }
                                    if ui.is_rect_visible(jit_hit_rect) {
                                        // One checkbox convention (round-19
                                        // issue 3). This site keeps its own
                                        // hit rect — it sits in a
                                        // hand-measured transport slot — but
                                        // the marks come from the shared
                                        // painter, so it can no longer drift
                                        // from ADV ENV / LINK / BYPASS /
                                        // SLD CHAIN. It used to invert the box
                                        // to an orange fill with a dark glyph.
                                        crate::gui::widgets::checkbox::paint_checkbox(
                                            ui,
                                            jit_hit_rect,
                                            jitter_val,
                                            "JITTER",
                                        );
                                    }

                                    let clear_btn = Button::new(
                                        nih_plug_egui::egui::RichText::new("CLEAR")
                                            .font(FontId::proportional(8.0))
                                            .strong()
                                            .color(theme::TEXT_PRIMARY),
                                    )
                                    .fill(theme::SURFACE_ELEVATED)
                                    .stroke(Stroke::new(1.0_f32, theme::BORDER_DEFAULT))
                                    // CLEAR fills the slot remainder after
                                    // JITTER and the 4-px gap, so Row D's
                                    // right edge lands on the shared column
                                    // edge like the other rows (round-13
                                    // issue 4, width corrected in round-19
                                    // issue 2).
                                    .min_size(vec2(TRANSPORT_SLOT_W - JIT_HIT_W - 4.0, row_h))
                                    .corner_radius(CornerRadius::same(2));

                                    if ui.add(clear_btn).on_hover_text("Reset all steps to default C3 notes").clicked() {
                                        if let Ok(mut pattern_guard) = params.pattern.write() {
                                            for step in pattern_guard.steps.iter_mut() {
                                                *step = SequencerStep {
                                                    semitone: 0,
                                                    octave: 0,
                                                    gate: StepGate::Note,
                                                    accent: false,
                                                    slide: false,
                                                    gate_len: 0.5,
                                                    velocity: 0.7,
                                                };
                                            }
                                        }
                                    }
                                },
                            );
                    });
                });
            });
        });
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_cells_fill_available_width_when_not_clamped() {
        // avail exactly fits 8 cells of 90 px + 7 gaps after transport + separator chrome
        let avail = TRANSPORT_COLUMN_W + SEPARATOR_CHROME_W + 8.0 * 90.0 + 7.0 * GRID_GAP;
        let (cell_w, clamped) = grid_cell_width(avail);
        assert!((cell_w - 90.0).abs() < 1e-4);
        assert!(!clamped);
    }

    #[test]
    fn grid_cell_width_clamps_to_minimum() {
        let (cell_w, clamped) = grid_cell_width(100.0);
        assert_eq!(cell_w, MIN_CELL_W);
        assert!(clamped);
    }

    #[test]
    fn transport_slots_fill_column_width() {
        // The two-slot grid spans the full transport column: slot + gap + slot.
        assert_eq!(TRANSPORT_SLOT_W * 2.0 + 4.0, TRANSPORT_COLUMN_W);
    }

    #[test]
    fn transport_row_height_fills_column_and_clamps() {
        // Title + rows + gaps consume the column exactly (no dead band).
        let avail = 268.0;
        let row_h = transport_row_height(avail);
        let total = TRANSPORT_TITLE_H
            + TRANSPORT_ROW_COUNT * row_h
            + TRANSPORT_ROW_COUNT * TRANSPORT_ROW_GAP;
        assert!((total - avail).abs() < 1e-4);
        // Tiny heights clamp to the minimum row height instead of going negative.
        assert_eq!(transport_row_height(0.0), TRANSPORT_ROW_MIN_H);
    }

    #[test]
    fn jitter_button_dimensions_accommodate_label() {
        // "JITTER" at 8 pt proportional is ~27 px wide; painted from a 15 px
        // offset. The hit rect must cover the full painted text plus margin so
        // no glyph pixels land on the CLEAR button's hit zone.
        const { assert!(JIT_HIT_W >= 15.0 + 27.0 + 2.0) };
    }

    #[test]
    fn tempo_snaps_to_integer() {
        // The BPM field displays `{:.0}`; the stored value must match the
        // display, so drags snap to whole BPM (PR#53 review).
        assert_eq!(snap_tempo(120.4), 120.0);
        assert_eq!(snap_tempo(120.5), 121.0);
        assert_eq!(snap_tempo(40.0), 40.0);
        assert_eq!(snap_tempo(299.9), 300.0);
    }

    #[test]
    fn row_d_clear_button_reaches_slot_right_edge() {
        // Row D's second slot is JITTER + gap + CLEAR; the CLEAR button fills
        // the slot remainder exactly, so the row's right edge lands on the
        // same column edge rows 1-4 use.
        //
        // Round-19 issue 2: the slot's item_spacing is 4, not the 3 this test
        // used to assume, so the old `- 1.0` width overshot the column edge by
        // 7 physical px. Slot = JITTER + gap + CLEAR, with nothing left over.
        let gap = 4.0;
        let clear_w = TRANSPORT_SLOT_W - JIT_HIT_W - gap;
        assert!(clear_w >= 36.0, "CLEAR must keep a usable hit target");
        assert_eq!(JIT_HIT_W + gap + clear_w, TRANSPORT_SLOT_W);
    }

    #[test]
    fn frame_min_height_adapts_and_clamps_to_floor() {
        let min_floor = GRID_TOTAL_H + 16.0; // 284.0
        assert_eq!(min_floor, 284.0);

        // When available height is smaller than floor + chrome, floor wins
        assert_eq!(frame_min_height(0.0), 284.0);
        assert_eq!(frame_min_height(100.0), 284.0);
        assert_eq!(frame_min_height(280.0), 284.0);
        assert_eq!(frame_min_height(280.0 + FRAME_VERT_CHROME_PX), 284.0);

        // When available height exceeds floor + chrome, chrome is subtracted
        let extra = 50.0;
        let avail = 284.0 + FRAME_VERT_CHROME_PX + extra;
        assert_eq!(frame_min_height(avail), 284.0 + extra);
    }
}
