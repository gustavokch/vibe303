use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{Color32, CornerRadius, Frame, Margin, Ui};
use std::sync::atomic::{AtomicU8, Ordering};

use crate::gui::theme;
use crate::gui::widgets::step_button::render_step_strip;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::Plugin303Params;

pub fn render_sequencer(
    ui: &mut Ui,
    params: &Plugin303Params,
    setter: &ParamSetter,
    current_step_atomic: &AtomicU8,
) {
    let active_step = current_step_atomic.load(Ordering::Relaxed);

    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.set_height(280.0);
            ui.set_width(1060.0);

            ui.horizontal(|ui| {
                // Left Transport & Config Column
                ui.vertical(|ui| {
                    ui.set_width(140.0);

                    ui.label(
                        nih_plug_egui::egui::RichText::new("SEQUENCER")
                            .font(nih_plug_egui::egui::FontId::new(12.0, nih_plug_egui::egui::FontFamily::Proportional))
                            .strong()
                            .color(theme::TEXT_PRIMARY),
                    );

                    ui.add_space(4.0);

                    // Run & Sync
                    ui.horizontal(|ui| {
                        let run_val = params.seq_run.value();
                        let run_btn = nih_plug_egui::egui::Button::new(if run_val { "■ STOP" } else { "▶ RUN" })
                            .fill(if run_val { theme::PLAYHEAD_RED } else { Color32::from_rgb(0x28, 0x30, 0x3c) });
                        if ui.add(run_btn).clicked() {
                            setter.begin_set_parameter(&params.seq_run);
                            setter.set_parameter(&params.seq_run, !run_val);
                            setter.end_set_parameter(&params.seq_run);
                        }

                        let sync_val = params.seq_sync.value();
                        let sync_btn = nih_plug_egui::egui::Button::new("SYNC")
                            .fill(if sync_val { theme::SLIDE_CYAN } else { Color32::from_rgb(0x28, 0x30, 0x3c) });
                        if ui.add(sync_btn).clicked() {
                            setter.begin_set_parameter(&params.seq_sync);
                            setter.set_parameter(&params.seq_sync, !sync_val);
                            setter.end_set_parameter(&params.seq_sync);
                        }
                    });

                    ui.add_space(4.0);
                    ui.add(Knob303::new(&params.seq_tempo, setter, "TEMPO", 13.0));

                    ui.add_space(4.0);
                    ui.add(SlideSwitch::new(
                        &params.seq_mode,
                        setter,
                        "MODE",
                        &["GRID", "303"],
                    ));

                    ui.add_space(4.0);
                    ui.add(SlideSwitch::new(
                        &params.seq_timebase,
                        setter,
                        "TIMEBASE",
                        &["16TH", "12TH"],
                    ));
                });

                ui.separator();

                // 16 Step Columns
                ui.horizontal(|ui| {
                    if let Ok(mut pattern_guard) = params.pattern.write() {
                        let num_steps = params.seq_length.value() as usize;

                        for step_idx in 0..16 {
                            let is_active = active_step as usize == step_idx;
                            let in_active_range = step_idx < num_steps;

                            ui.group(|ui| {
                                ui.add_enabled_ui(in_active_range, |ui| {
                                    render_step_strip(
                                        ui,
                                        step_idx,
                                        &mut pattern_guard.steps[step_idx],
                                        is_active,
                                    );
                                });
                            });
                        }
                    }
                });
            });
        });
}
