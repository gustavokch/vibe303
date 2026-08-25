use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    pos2, vec2, Button, ComboBox, CornerRadius, DragValue, FontId, Frame, Margin, Sense,
    Stroke, StrokeKind, Ui,
};
use std::sync::atomic::{AtomicU8, Ordering};

use crate::gui::theme;
use crate::gui::widgets::piano_roll_popup::render_piano_roll_popup;
use crate::gui::widgets::render_step_strip;
use crate::gui::widgets::step_button::STRIP_HEIGHT;
use crate::params::{Plugin303Params, SequencerStep, StepGate};

/// Width reserved for the transport/pattern column right of the step grid.
const TRANSPORT_COLUMN_W: f32 = 280.0;
/// Horizontal gap between adjacent step cells (and grid rows vertically).
const GRID_GAP: f32 = 4.0;
/// `add_space(8)` + separator (~2) + `add_space(8)` between grid and transport column.
const SEPARATOR_CHROME_W: f32 = 18.0;
/// Smallest usable step cell; below this the grid overflows instead of scrolling.
const MIN_CELL_W: f32 = 72.0;
/// Vertical chrome of the panel frame: inner_margin top (4) + border stroke
/// (~2x1) + layout slack, subtracted from the pre-frame available height so
/// the frame bottom reaches the window bottom.
const FRAME_VERT_CHROME_PX: f32 = 8.0;

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

    ui.horizontal(|ui| {
        super::generator_panel::render_generator_panel(ui, params, gen_params, setter);
    });
    ui.add_space(0.5);

    let frame_avail_h = ui.available_height();

    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin {
            left: 8,
            right: 8,
            top: 4,
            bottom: 0,
        })
        .show(ui, |ui| {
            ui.set_min_height((frame_avail_h - FRAME_VERT_CHROME_PX).max(280.0));
            ui.set_max_width(1048.0);

            // Grid geometry: 2 rows x 8 columns; transport column on the right
            let (cell_w, clamped) = grid_cell_width(ui.available_width());
            debug_assert!(
                !clamped,
                "sequencer grid overflows available width; cells clamp to {MIN_CELL_W}px and squeeze the transport column"
            );
            let cell_h = STRIP_HEIGHT;

            ui.horizontal(|ui| {
                // 1. LEFT / MAIN: step pitch key columns in a 2x8 grid (steps 01-08 top, 09-16 bottom)
                ui.vertical(|ui| {
                    if let Ok(mut pattern_guard) = params.pattern.write() {
                        let is_running = params.seq_run.value();
                        ui.spacing_mut().item_spacing = vec2(4.0, 12.0);

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
                    ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
                    ui.spacing_mut().button_padding = vec2(3.0, 1.0);

                    ui.label(
                        nih_plug_egui::egui::RichText::new("TRANSPORT & PATTERN")
                            .font(FontId::proportional(9.0))
                            .strong()
                            .color(theme::TEXT_PRIMARY),
                    );

                    // Row A: Run/Sync Buttons
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
                            .color(theme::TEXT_ON_ACCENT),
                        )
                        .fill(theme::ACCENT_ORANGE)
                        .min_size(vec2(72.0, 24.0))
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
                            .color(if sync_val { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY }),
                        )
                        .fill(if sync_val {
                            theme::SURFACE_ELEVATED
                        } else {
                            theme::WELL_BG
                        })
                        .stroke(Stroke::new(1.0_f32, if sync_val { theme::ACCENT_ORANGE } else { theme::BORDER_DEFAULT }))
                        .min_size(vec2(84.0, 24.0))
                        .corner_radius(CornerRadius::same(2));

                        if ui.add(sync_btn).clicked() {
                            setter.begin_set_parameter(&params.seq_sync);
                            setter.set_parameter(&params.seq_sync, !sync_val);
                            setter.end_set_parameter(&params.seq_sync);
                        }
                    });

                    // Row: MIDI Out Toggle + Channel + Synth Bypass
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 0.0);

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
                        .min_size(vec2(60.0, 24.0))
                        .corner_radius(CornerRadius::same(2));

                        if ui.add(midi_btn).on_hover_text("Toggle MIDI Output from 16-step sequencer").clicked() {
                            setter.begin_set_parameter(&params.midi_out);
                            setter.set_parameter(&params.midi_out, !midi_val);
                            setter.end_set_parameter(&params.midi_out);
                        }

                        // MIDI Channel selector
                        ui.label(
                            nih_plug_egui::egui::RichText::new("CH:")
                                .font(FontId::proportional(8.0))
                                .color(theme::TEXT_MUTED),
                        );
                        let mut ch_val = params.midi_out_channel.value();
                        if ui
                            .add(
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

                        ui.add_space(2.0);

                        // SYNTH BYPASS button
                        let bypass_val = params.synth_bypass.value();
                        let bypass_btn = Button::new(
                            nih_plug_egui::egui::RichText::new(if bypass_val { "SYNTH: MUTE" } else { "SYNTH: ON" })
                                .font(FontId::proportional(8.5))
                                .strong()
                                .color(if bypass_val { theme::ACCENT_RUBY } else { theme::TEXT_PRIMARY }),
                        )
                        .fill(if bypass_val { theme::SURFACE_ELEVATED } else { theme::WELL_BG })
                        .stroke(Stroke::new(1.0_f32, if bypass_val { theme::ACCENT_RUBY } else { theme::BORDER_DEFAULT }))
                        .min_size(vec2(72.0, 24.0))
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
                        .min_size(vec2(78.0, 24.0))
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
                        .min_size(vec2(78.0, 24.0))
                        .corner_radius(CornerRadius::same(2));

                        if ui.add(shift_right_btn).on_hover_text("Rotate pattern steps right (cyclic)").clicked() {
                            if let Ok(mut pattern_guard) = params.pattern.write() {
                                pattern_guard.steps.rotate_right(1);
                            }
                        }
                    });

                    // Row C: BPM + Length (BPM -/+ steppers with >= 24px hit targets)
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
                        ui.label(
                            nih_plug_egui::egui::RichText::new("BPM:")
                                .font(FontId::proportional(8.0))
                                .color(theme::TEXT_MUTED),
                        );
                        let mut tempo_val = params.seq_tempo.value();

                        let dn_btn = Button::new("-")
                            .fill(theme::WELL_BG)
                            .stroke(Stroke::new(1.0_f32, theme::BORDER_DEFAULT))
                            .min_size(vec2(24.0, 24.0))
                            .corner_radius(CornerRadius::same(2));
                        if ui.add(dn_btn).clicked() {
                            setter.begin_set_parameter(&params.seq_tempo);
                            setter.set_parameter(
                                &params.seq_tempo,
                                (tempo_val - 1.0).clamp(40.0, 300.0),
                            );
                            setter.end_set_parameter(&params.seq_tempo);
                        }

                        if ui
                            .add(
                                DragValue::new(&mut tempo_val)
                                    .range(40.0..=300.0)
                                    .speed(0.1)
                                    .custom_formatter(|n, _| format!("{:.1}", n)),
                            )
                            .changed()
                        {
                            setter.begin_set_parameter(&params.seq_tempo);
                            setter.set_parameter(&params.seq_tempo, tempo_val);
                            setter.end_set_parameter(&params.seq_tempo);
                        }

                        let up_btn = Button::new("+")
                            .fill(theme::WELL_BG)
                            .stroke(Stroke::new(1.0_f32, theme::BORDER_DEFAULT))
                            .min_size(vec2(24.0, 24.0))
                            .corner_radius(CornerRadius::same(2));
                        if ui.add(up_btn).clicked() {
                            setter.begin_set_parameter(&params.seq_tempo);
                            setter.set_parameter(
                                &params.seq_tempo,
                                (tempo_val + 1.0).clamp(40.0, 300.0),
                            );
                            setter.end_set_parameter(&params.seq_tempo);
                        }

                        ui.add_space(3.0);

                        ui.label(
                            nih_plug_egui::egui::RichText::new("LEN:")
                                .font(FontId::proportional(8.0))
                                .color(theme::TEXT_MUTED),
                        );
                        let mut len_val = params.seq_length.value();
                        if ui
                            .add(
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
                    });

                    // Row D: Timebase + Mode + Jitter + Clear (enlarged hit areas >= 24px)
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
                        ui.spacing_mut().button_padding = vec2(4.0, 3.0);

                        let tb_val = params.seq_timebase.value();
                        ComboBox::from_id_salt("seq_tb_combo")
                            .selected_text(match tb_val {
                                crate::params::Timebase::Straight => "16th",
                                crate::params::Timebase::Triplet => "12th",
                            })
                            .width(36.0)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(
                                        tb_val == crate::params::Timebase::Straight,
                                        "16th",
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
                                        "12th",
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
                            .selected_text(match mode_val {
                                crate::params::SequencerMode::DirectGrid => "Direct",
                                crate::params::SequencerMode::DecoupledHardware => "Decoupled",
                            })
                            .width(54.0)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(
                                        mode_val == crate::params::SequencerMode::DirectGrid,
                                        "Direct",
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
                                        "Decoupled",
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

                        let mut jitter_val = params.seq_jitter.value();
                        let (jit_hit_rect, jit_resp) = ui.allocate_exact_size(vec2(36.0, 28.0), Sense::click());
                        if jit_resp.clicked() {
                            jitter_val = !jitter_val;
                            setter.begin_set_parameter(&params.seq_jitter);
                            setter.set_parameter(&params.seq_jitter, jitter_val);
                            setter.end_set_parameter(&params.seq_jitter);
                        }
                        if ui.is_rect_visible(jit_hit_rect) {
                            let p = ui.painter();
                            let box_rect = nih_plug_egui::egui::Rect::from_center_size(pos2(jit_hit_rect.left() + 6.0, jit_hit_rect.center().y), vec2(12.0, 12.0));
                            p.rect_filled(box_rect, CornerRadius::same(2), if jitter_val { theme::ACCENT_ORANGE } else { theme::WELL_BG });
                            p.rect_stroke(box_rect, CornerRadius::same(2), Stroke::new(1.0_f32, if jitter_val { theme::ACCENT_ORANGE } else { theme::BORDER_DEFAULT }), StrokeKind::Inside);
                            p.text(pos2(jit_hit_rect.left() + 15.0, jit_hit_rect.center().y), nih_plug_egui::egui::Align2::LEFT_CENTER, "JIT", FontId::proportional(8.0), if jitter_val { theme::TEXT_PRIMARY } else { theme::TEXT_MUTED });
                        }

                        let clear_btn = Button::new(
                            nih_plug_egui::egui::RichText::new("CLEAR")
                                .font(FontId::proportional(8.0))
                                .strong()
                                .color(theme::TEXT_PRIMARY),
                        )
                        .fill(theme::SURFACE_ELEVATED)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER_DEFAULT))
                        .min_size(vec2(36.0, 24.0))
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
                    });
                });
            });
        });
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
    fn frame_vertical_chrome_covers_margin_and_stroke() {
        // inner_margin top (4) + border stroke (~2x1) + layout slack
        assert_eq!(FRAME_VERT_CHROME_PX, 8.0);
    }
}
