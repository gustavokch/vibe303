use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    vec2, Color32, ComboBox, CornerRadius, DragValue, FontId, Frame, Margin, Ui,
};
use std::sync::atomic::{AtomicU8, Ordering};

use crate::gui::theme;
use crate::gui::widgets::piano_roll_popup::render_piano_roll_popup;
use crate::gui::widgets::render_step_strip;
use crate::params::Plugin303Params;

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
    ui.add_space(2.0);

    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin {
            left: 8,
            right: 8,
            top: 4,
            bottom: 6,
        })
        .show(ui, |ui| {
            ui.set_max_width(1048.0);

            ui.horizontal(|ui| {
                // 1. LEFT / MAIN: 16-Step Pitch Key Columns
                ui.vertical(|ui| {
                    if let Ok(mut pattern_guard) = params.pattern.write() {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                            let is_running = params.seq_run.value();

                            for step_idx in 0..16 {
                                let is_current = active_step as usize == step_idx;
                                let step = &mut pattern_guard.steps[step_idx];

                                render_step_strip(ui, step_idx, step, is_current, is_running);
                            }
                        });

                        render_piano_roll_popup(ui.ctx(), &mut pattern_guard);
                    }
                });

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);

                // 2. RIGHT COLUMN: Compact Transport & Master
                ui.vertical(|ui| {
                    ui.set_width(190.0);

                    // Row A: Run/Sync
                    ui.horizontal(|ui| {
                        let run_val = params.seq_run.value();
                        let run_btn = nih_plug_egui::egui::Button::new(
                            nih_plug_egui::egui::RichText::new(if run_val {
                                "■ STOP"
                            } else {
                                "▶ RUN"
                            })
                            .font(FontId::proportional(10.0))
                            .strong()
                            .color(Color32::BLACK),
                        )
                        .fill(if run_val {
                            theme::PLAYHEAD_RED
                        } else {
                            theme::SOUPED_ORANGE
                        })
                        .min_size(vec2(80.0, 18.0));

                        if ui.add(run_btn).clicked() {
                            setter.begin_set_parameter(&params.seq_run);
                            setter.set_parameter(&params.seq_run, !run_val);
                            setter.end_set_parameter(&params.seq_run);
                        }

                        let sync_val = params.seq_sync.value();
                        let sync_btn = nih_plug_egui::egui::Button::new(
                            nih_plug_egui::egui::RichText::new(if sync_val {
                                "HOST"
                            } else {
                                "INT"
                            })
                            .font(FontId::proportional(9.0))
                            .strong()
                            .color(theme::TEXT_PRIMARY),
                        )
                        .fill(if sync_val {
                            theme::BTN_ACTIVE_BG
                        } else {
                            theme::BTN_INACTIVE_BG
                        })
                        .min_size(vec2(50.0, 18.0));

                        if ui.add(sync_btn).clicked() {
                            setter.begin_set_parameter(&params.seq_sync);
                            setter.set_parameter(&params.seq_sync, !sync_val);
                            setter.end_set_parameter(&params.seq_sync);
                        }
                    });

                    ui.add_space(1.0);

                    // Row B: BPM + Length
                    ui.horizontal(|ui| {
                        ui.label(
                            nih_plug_egui::egui::RichText::new("BPM:")
                                .font(FontId::proportional(9.0))
                                .color(theme::TEXT_SECONDARY),
                        );
                        let mut tempo_val = params.seq_tempo.value();

                        let dn_btn = nih_plug_egui::egui::Button::new("-")
                            .fill(theme::BTN_INACTIVE_BG)
                            .stroke(nih_plug_egui::egui::Stroke::new(
                                1.0_f32,
                                theme::PANEL_BORDER,
                            ))
                            .min_size(vec2(12.0, 16.0));
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

                        let up_btn = nih_plug_egui::egui::Button::new("+")
                            .fill(theme::BTN_INACTIVE_BG)
                            .stroke(nih_plug_egui::egui::Stroke::new(
                                1.0_f32,
                                theme::PANEL_BORDER,
                            ))
                            .min_size(vec2(12.0, 16.0));
                        if ui.add(up_btn).clicked() {
                            setter.begin_set_parameter(&params.seq_tempo);
                            setter.set_parameter(
                                &params.seq_tempo,
                                (tempo_val + 1.0).clamp(40.0, 300.0),
                            );
                            setter.end_set_parameter(&params.seq_tempo);
                        }

                        ui.add_space(2.0);

                        ui.label(
                            nih_plug_egui::egui::RichText::new("LEN:")
                                .font(FontId::proportional(9.0))
                                .color(theme::TEXT_SECONDARY),
                        );
                        let mut len_val = params.seq_length.value();
                        if ui
                            .add(DragValue::new(&mut len_val).range(1..=16))
                            .changed()
                        {
                            setter.begin_set_parameter(&params.seq_length);
                            setter.set_parameter(&params.seq_length, len_val);
                            setter.end_set_parameter(&params.seq_length);
                        }
                    });

                    ui.add_space(1.0);

                    // Row C: Timebase + Mode
                    ui.horizontal(|ui| {
                        let tb_val = params.seq_timebase.value();
                        ComboBox::from_id_salt("seq_tb_combo")
                            .selected_text(match tb_val {
                                crate::params::Timebase::Straight => "16th",
                                crate::params::Timebase::Triplet => "12th",
                            })
                            .width(42.0)
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

                        ui.add_space(2.0);

                        let mode_val = params.seq_mode.value();
                        ComboBox::from_id_salt("seq_mode_combo")
                            .selected_text(match mode_val {
                                crate::params::SequencerMode::DirectGrid => "Direct",
                                crate::params::SequencerMode::DecoupledHardware => "Decoupled",
                            })
                            .width(62.0)
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

                        ui.add_space(2.0);

                        let mut jitter_val = params.seq_jitter.value();
                        if ui.checkbox(&mut jitter_val, "JIT").changed() {
                            setter.begin_set_parameter(&params.seq_jitter);
                            setter.set_parameter(&params.seq_jitter, jitter_val);
                            setter.end_set_parameter(&params.seq_jitter);
                        }
                    });
                });
            });
        });
}
