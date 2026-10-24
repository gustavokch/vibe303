use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    epaint::Shape, style::WidgetVisuals, vec2, AboveOrBelow, Align, Button, ComboBox, DragValue,
    FontId, Layout, Rect, RichText, Stroke, Ui,
};
use std::sync::atomic::{AtomicU8, Ordering};

use crate::gui::theme;
use crate::gui::widgets::panel::{DevicePanel, HEADER_H as PANEL_HEADER_H, PAD_Y as PANEL_PAD_Y};
use crate::gui::widgets::piano_roll_popup::render_piano_roll_popup;
use crate::gui::widgets::step_button::{render_step_strip, STRIP_HEIGHT};
use crate::gui::widgets::toggle::ToggleButton;
use crate::params::{Plugin303Params, SequencerStep, StepGate};

/// Horizontal gap between adjacent step cells and grid rows vertically.
pub const GRID_GAP: f32 = 3.0;

/// Total device height for the sequencer and transport cards:
/// 2 rows x STRIP_HEIGHT (198) + row gap (3) + header (22) + vertical padding (8) = 231.
pub const SEQUENCER_H: f32 = 2.0 * STRIP_HEIGHT + GRID_GAP + PANEL_HEADER_H + 2.0 * PANEL_PAD_Y;

/// Standard height of flat control tiles in the transport panel.
const ROW_H: f32 = 28.0;
/// Horizontal gap between the two slots in each transport row.
const SLOT_GAP: f32 = 3.0;

/// How long a first CLEAR click stays armed, in seconds.
const CLEAR_CONFIRM_S: f64 = 3.0;

/// Whether CLEAR, armed at `armed_at`, is still armed at `now`.
fn clear_armed(armed_at: Option<f64>, now: f64) -> bool {
    armed_at.is_some_and(|t| now - t <= CLEAR_CONFIRM_S)
}

/// Custom ComboBox icon painter that always paints a downward-pointing triangle (▼).
/// These bottom-row dropdowns sit low enough in the window that egui flips the glyph
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

/// Snap a dragged BPM value to a whole number so the `{:.0}` display always
/// matches the stored value (PR#53 review).
fn snap_tempo(v: f32) -> f32 {
    v.round()
}

/// 16-step sequencer pattern grid: 2 rows x 8 step strips.
pub fn render_sequencer_grid(ui: &mut Ui, params: &Plugin303Params, current_step: &AtomicU8) {
    DevicePanel::new("SEQUENCER", SEQUENCER_H).show(
        ui,
        |_header_ui| {},
        |ui| {
            let body_w = ui.available_width();
            let cell_w = (body_w - 7.0 * GRID_GAP) / 8.0;
            let active_step = current_step.load(Ordering::Relaxed);
            let is_running = params.seq_run.value();

            if let Ok(mut pattern_guard) = params.pattern.write() {
                ui.spacing_mut().item_spacing = vec2(GRID_GAP, GRID_GAP);

                for row in 0..2 {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(GRID_GAP, 0.0);

                        for col in 0..8 {
                            let step_idx = row * 8 + col;
                            let is_current = active_step as usize == step_idx;
                            let step = &mut pattern_guard.steps[step_idx];

                            render_step_strip(
                                ui,
                                step_idx,
                                step,
                                is_current,
                                is_running,
                                cell_w,
                                STRIP_HEIGHT,
                            );
                        }
                    });
                }

                render_piano_roll_popup(ui.ctx(), &mut pattern_guard);
            }
        },
    );
}

/// Transport and pattern controls panel: 6 evenly distributed rows of flat tiles.
pub fn render_transport_panel(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    DevicePanel::new("TRANSPORT & PATTERN", SEQUENCER_H).show(
        ui,
        |_header_ui| {},
        |ui| {
            let body_w = ui.available_width();
            let slot_w = ((body_w - SLOT_GAP) * 0.5).floor();
            let row_gap = ((ui.available_height() - 6.0 * ROW_H) / 5.0).max(2.0);

            // Row 1: RUN and SYNC
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(SLOT_GAP, 0.0);

                let mut run = params.seq_run.value();
                if ui
                    .add(
                        ToggleButton::new(&mut run, "RUN")
                            .min_width(slot_w)
                            .height(ROW_H),
                    )
                    .on_hover_text("Lit: the sequencer runs (with SYNC lit, while the host plays).")
                    .changed()
                {
                    setter.begin_set_parameter(&params.seq_run);
                    setter.set_parameter(&params.seq_run, run);
                    setter.end_set_parameter(&params.seq_run);
                }

                let mut sync = params.seq_sync.value();
                if ui
                    .add(
                        ToggleButton::new(&mut sync, "SYNC")
                            .min_width(slot_w)
                            .height(ROW_H),
                    )
                    .on_hover_text(
                        "Lit: follow the host transport and tempo. Unlit: internal clock at BPM.",
                    )
                    .changed()
                {
                    setter.begin_set_parameter(&params.seq_sync);
                    setter.set_parameter(&params.seq_sync, sync);
                    setter.end_set_parameter(&params.seq_sync);
                }
            });

            ui.add_space(row_gap);

            // Row 2: MIDI + Channel and SYNTH
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(SLOT_GAP, 0.0);

                // Left slot: MIDI toggle + Channel field
                ui.allocate_ui_with_layout(
                    vec2(slot_w, ROW_H),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 0.0);

                        let mut midi = params.midi_out.value();
                        let midi_btn_w = (slot_w * 0.55).floor();
                        if ui
                            .add(
                                ToggleButton::new(&mut midi, "MIDI")
                                    .min_width(midi_btn_w)
                                    .height(ROW_H),
                            )
                            .on_hover_text("Lit: send the sequence as MIDI notes on CH.")
                            .changed()
                        {
                            setter.begin_set_parameter(&params.midi_out);
                            setter.set_parameter(&params.midi_out, midi);
                            setter.end_set_parameter(&params.midi_out);
                        }

                        ui.label(
                            RichText::new("CH:")
                                .font(FontId::proportional(8.0))
                                .color(theme::TEXT_SECONDARY),
                        );

                        let mut ch_val = params.midi_out_channel.value();
                        let ch_w = ui.available_width();
                        ui.scope(|ui| {
                            theme::field_style(ui);
                            if ui
                                .add_sized(
                                    vec2(ch_w, ROW_H),
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
                        });
                    },
                );

                // Right slot: SYNTH voice on/off
                let mut synth_on = !params.synth_bypass.value();
                if ui
                    .add(
                        ToggleButton::new(&mut synth_on, "SYNTH")
                            .min_width(slot_w)
                            .height(ROW_H),
                    )
                    .on_hover_text("Lit: the internal voice plays. Unlit: muted (silence, 0% CPU).")
                    .changed()
                {
                    setter.begin_set_parameter(&params.synth_bypass);
                    setter.set_parameter(&params.synth_bypass, !synth_on);
                    setter.end_set_parameter(&params.synth_bypass);
                }
            });

            ui.add_space(row_gap);

            // Row 3: Cyclic Pattern Shift Controls (< SHIFT and > SHIFT)
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(SLOT_GAP, 0.0);

                let shift_left_btn = Button::new(
                    RichText::new("< SHIFT")
                        .font(FontId::proportional(8.5))
                        .color(theme::TEXT_PRIMARY),
                )
                .fill(theme::SURFACE_ELEVATED)
                .min_size(vec2(slot_w, ROW_H));

                if ui
                    .add(shift_left_btn)
                    .on_hover_text("Rotate pattern steps left (cyclic)")
                    .clicked()
                {
                    if let Ok(mut pattern_guard) = params.pattern.write() {
                        pattern_guard.steps.rotate_left(1);
                    }
                }

                let shift_right_btn = Button::new(
                    RichText::new("> SHIFT")
                        .font(FontId::proportional(8.5))
                        .color(theme::TEXT_PRIMARY),
                )
                .fill(theme::SURFACE_ELEVATED)
                .min_size(vec2(slot_w, ROW_H));

                if ui
                    .add(shift_right_btn)
                    .on_hover_text("Rotate pattern steps right (cyclic)")
                    .clicked()
                {
                    if let Ok(mut pattern_guard) = params.pattern.write() {
                        pattern_guard.steps.rotate_right(1);
                    }
                }
            });

            ui.add_space(row_gap);

            // Row 4: BPM -/field/+ and LEN -/field/+
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(SLOT_GAP, 0.0);

                // BPM slot
                ui.allocate_ui_with_layout(
                    vec2(slot_w, ROW_H),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing = vec2(3.0, 0.0);

                        ui.label(
                            RichText::new("BPM:")
                                .font(FontId::proportional(8.0))
                                .color(theme::TEXT_SECONDARY),
                        );
                        let mut tempo_val = params.seq_tempo.value();

                        let dn_btn = Button::new(
                            RichText::new("-")
                                .font(FontId::proportional(9.0))
                                .color(theme::TEXT_PRIMARY),
                        )
                        .fill(theme::SURFACE_ELEVATED)
                        .min_size(vec2(22.0, ROW_H));
                        if ui.add(dn_btn).clicked() {
                            setter.begin_set_parameter(&params.seq_tempo);
                            setter.set_parameter(
                                &params.seq_tempo,
                                (tempo_val - 1.0).clamp(40.0, 300.0),
                            );
                            setter.end_set_parameter(&params.seq_tempo);
                        }

                        let dv_w = (ui.available_width() - 25.0).max(20.0);
                        ui.scope(|ui| {
                            theme::field_style(ui);
                            if ui
                                .add_sized(
                                    vec2(dv_w, ROW_H),
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
                        });

                        let up_btn = Button::new(
                            RichText::new("+")
                                .font(FontId::proportional(9.0))
                                .color(theme::TEXT_PRIMARY),
                        )
                        .fill(theme::SURFACE_ELEVATED)
                        .min_size(vec2(22.0, ROW_H));
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

                // LEN slot
                ui.allocate_ui_with_layout(
                    vec2(slot_w, ROW_H),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing = vec2(3.0, 0.0);

                        ui.label(
                            RichText::new("LEN:")
                                .font(FontId::proportional(8.0))
                                .color(theme::TEXT_SECONDARY),
                        );
                        let mut len_val = params.seq_length.value();

                        let dn_btn = Button::new(
                            RichText::new("-")
                                .font(FontId::proportional(9.0))
                                .color(theme::TEXT_PRIMARY),
                        )
                        .fill(theme::SURFACE_ELEVATED)
                        .min_size(vec2(22.0, ROW_H));
                        if ui.add(dn_btn).clicked() {
                            setter.begin_set_parameter(&params.seq_length);
                            setter.set_parameter(&params.seq_length, (len_val - 1).clamp(1, 16));
                            setter.end_set_parameter(&params.seq_length);
                        }

                        let dv_w = (ui.available_width() - 25.0).max(20.0);
                        ui.scope(|ui| {
                            theme::field_style(ui);
                            if ui
                                .add_sized(
                                    vec2(dv_w, ROW_H),
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

                        let up_btn = Button::new(
                            RichText::new("+")
                                .font(FontId::proportional(9.0))
                                .color(theme::TEXT_PRIMARY),
                        )
                        .fill(theme::SURFACE_ELEVATED)
                        .min_size(vec2(22.0, ROW_H));
                        if ui.add(up_btn).clicked() {
                            setter.begin_set_parameter(&params.seq_length);
                            setter.set_parameter(&params.seq_length, (len_val + 1).clamp(1, 16));
                            setter.end_set_parameter(&params.seq_length);
                        }
                    },
                );
            });

            ui.add_space(row_gap);

            // Row 5: Timebase (16th/12th) and Mode (Direct/Decoupled) Dropdowns
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(SLOT_GAP, 0.0);

                // Timebase combo
                ui.allocate_ui_with_layout(
                    vec2(slot_w, ROW_H),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.scope(|ui| {
                            theme::field_style(ui);
                            ui.spacing_mut().interact_size.y = ROW_H;
                            let tb_val = params.seq_timebase.value();
                            ComboBox::from_id_salt("seq_tb_combo")
                                .icon(paint_combobox_down_arrow)
                                .selected_text(
                                    RichText::new(match tb_val {
                                        crate::params::Timebase::Straight => "16th",
                                        crate::params::Timebase::Triplet => "12th",
                                    })
                                    .font(FontId::proportional(8.5)),
                                )
                                .width(slot_w)
                                .show_ui(ui, |ui| {
                                    if ui
                                        .selectable_label(
                                            tb_val == crate::params::Timebase::Straight,
                                            RichText::new("16th").font(FontId::proportional(8.5)),
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
                                            RichText::new("12th").font(FontId::proportional(8.5)),
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
                        });
                    },
                );

                // Mode combo
                ui.allocate_ui_with_layout(
                    vec2(slot_w, ROW_H),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.scope(|ui| {
                            theme::field_style(ui);
                            ui.spacing_mut().interact_size.y = ROW_H;
                            let mode_val = params.seq_mode.value();
                            ComboBox::from_id_salt("seq_mode_combo")
                                .icon(paint_combobox_down_arrow)
                                .selected_text(
                                    RichText::new(match mode_val {
                                        crate::params::SequencerMode::DirectGrid => "Direct",
                                        crate::params::SequencerMode::DecoupledHardware => {
                                            "Decoupled"
                                        }
                                    })
                                    .font(FontId::proportional(8.5)),
                                )
                                .width(slot_w)
                                .show_ui(ui, |ui| {
                                    if ui
                                        .selectable_label(
                                            mode_val == crate::params::SequencerMode::DirectGrid,
                                            RichText::new("Direct").font(FontId::proportional(8.5)),
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
                                            RichText::new("Decoupled")
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
                        });
                    },
                );
            });

            ui.add_space(row_gap);

            // Row 6: JITTER ToggleButton and CLEAR Button
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(SLOT_GAP, 0.0);

                ui.allocate_ui_with_layout(
                    vec2(slot_w, ROW_H),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        let mut jitter_val = params.seq_jitter.value();
                        if ui
                            .add(
                                ToggleButton::new(&mut jitter_val, "JITTER")
                                    .min_width(slot_w)
                                    .height(ROW_H),
                            )
                            .on_hover_text("Clock jitter / micro-timing humanization")
                            .changed()
                        {
                            setter.begin_set_parameter(&params.seq_jitter);
                            setter.set_parameter(&params.seq_jitter, jitter_val);
                            setter.end_set_parameter(&params.seq_jitter);
                        }
                    },
                );

                // CLEAR wipes the pattern: the first click arms it, a second
                // click within CLEAR_CONFIRM_S clears (round-20 #9).
                let clear_id = ui.id().with("clear_armed_at");
                let now = ui.input(|i| i.time);
                let armed_at = ui.ctx().data(|d| d.get_temp::<f64>(clear_id));
                let armed = clear_armed(armed_at, now);
                let clear_btn = Button::new(
                    RichText::new(if armed { "CONFIRM CLEAR" } else { "CLEAR" })
                        .font(FontId::proportional(8.5))
                        .color(theme::TEXT_PRIMARY),
                )
                .fill(if armed {
                    theme::SURFACE_HOVER
                } else {
                    theme::SURFACE_ELEVATED
                })
                .min_size(vec2(slot_w, ROW_H));

                if ui
                    .add(clear_btn)
                    .on_hover_text("Reset all 16 steps to C3 notes. Click twice to confirm.")
                    .clicked()
                {
                    if armed {
                        ui.ctx().data_mut(|d| d.remove::<f64>(clear_id));
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
                    } else {
                        ui.ctx().data_mut(|d| d.insert_temp(clear_id, now));
                    }
                }
            });
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tempo_snaps_to_integer() {
        // The BPM field displays `{:.0}`; the stored value must match the
        // display, so drags snap to whole BPM (PR#53 review).
        assert_eq!(snap_tempo(120.4), 120.0);
        assert_eq!(snap_tempo(120.5), 121.0);
        assert_eq!(snap_tempo(40.0), 40.0);
        assert_eq!(snap_tempo(299.9), 300.0);
    }

    /// CLEAR wipes all 16 steps, so one click only arms it; a second click
    /// within CLEAR_CONFIRM_S clears. After the window the next click re-arms.
    #[test]
    fn clear_confirms_within_the_window_only() {
        assert!(!clear_armed(None, 10.0), "a first click must only arm");
        assert!(clear_armed(Some(10.0), 12.0));
        assert!(
            clear_armed(Some(10.0), 10.0 + CLEAR_CONFIRM_S),
            "boundary is inclusive"
        );
        assert!(
            !clear_armed(Some(10.0), 13.5),
            "an expired arm must not clear"
        );
    }
}
