pub mod panels;
pub mod theme;
pub mod widgets;

use nih_plug::prelude::Editor;
use nih_plug_egui::egui::{Frame, Margin};
use nih_plug_egui::{create_egui_editor, EguiState};
use std::sync::atomic::AtomicU8;
use std::sync::Arc;

use crate::params::Plugin303Params;

pub const GUI_WIDTH: u32 = 1366;
pub const GUI_HEIGHT: u32 = 960;

/// Uniform inter-panel gutter in logical px. Every gap in the central panel
/// stack is an explicit `add_space(PANEL_GAP)` with the automatic
/// `item_spacing.y` zeroed, so all six gutters measure identically
/// (round-13 issue 11).
pub(crate) const PANEL_GAP: f32 = 4.0;

pub fn create_editor(
    params: Arc<Plugin303Params>,
    editor_state: Arc<EguiState>,
    current_playing_step: Arc<AtomicU8>,
) -> Option<Box<dyn Editor>> {
    let gen_params = if let Ok(s) = std::env::var("VIBE303_QA_STATE") {
        if s == "generator" {
            crate::dsp::generator::GeneratorParams {
                root_note: 3, // D#
                scale: crate::dsp::generator::MusicalScale::Phrygian,
                octave_min: 0,
                octave_max: 1,
                rhythm_pulses: 7,
                rhythm_length: 12,
                rhythm_shift: 3,
                pitch_length: 9,
                rest_probability: 0.15,
                accent_probability: 0.45,
                slide_probability: 0.35,
                octave_up_prob: 0.3,
                octave_down_prob: 0.2,
                accent_pitch_bias: 0.7,
                slide_chaining: true,
                mutation_rate: 0.25,
                seed: 1337,
            }
        } else {
            crate::dsp::generator::GeneratorParams::default()
        }
    } else {
        crate::dsp::generator::GeneratorParams::default()
    };
    let gen_params_state = Arc::new(std::sync::Mutex::new(gen_params));
    // Read once at editor creation: the closure below runs every frame, and a
    // per-frame `env::var` is a syscall + alloc at repaint rate.
    let qa_zoom = matches!(
        std::env::var("VIBE303_QA_STATE").as_deref(),
        Ok("zoom75")
    );

    create_egui_editor(
        editor_state,
        (),
        move |egui_ctx, _| {
            if qa_zoom {
                egui_ctx.set_zoom_factor(0.75);
            }
            egui_ctx.send_viewport_cmd(nih_plug_egui::egui::ViewportCommand::Title(
                "TECHNEO 303".into(),
            ));
            egui_ctx.set_visuals(nih_plug_egui::egui::Visuals::dark());
            // One font rule (round-11 item 12): every numeric readout — every
            // DragValue — renders in the monospace face at the compact size;
            // word-valued controls keep the proportional face.
            egui_ctx.style_mut(|style| {
                style.text_styles.insert(
                    nih_plug_egui::egui::TextStyle::Monospace,
                    nih_plug_egui::egui::FontId::monospace(9.0),
                );
                style.drag_value_text_style = nih_plug_egui::egui::TextStyle::Monospace;

                // Widget text defaults to egui's greys (#B4B4B4 inactive,
                // #999999 on combo buttons — neither is a theme token, and the
                // combo value measured 3.87:1). Point them at the palette so
                // every DragValue, Button and ComboBox that does not set its
                // own colour still lands on a token (round-14 issue 7).
                let widgets = &mut style.visuals.widgets;
                widgets.noninteractive.fg_stroke.color = theme::TEXT_SECONDARY;
                widgets.inactive.fg_stroke.color = theme::TEXT_SECONDARY;
                widgets.hovered.fg_stroke.color = theme::TEXT_PRIMARY;
                widgets.active.fg_stroke.color = theme::TEXT_PRIMARY;
                widgets.open.fg_stroke.color = theme::TEXT_PRIMARY;
            });
        },
        move |egui_ctx, setter, _state| {
            nih_plug_egui::egui::CentralPanel::default()
                .frame(
                    Frame::new()
                        .fill(theme::CHASSIS_BG)
                        .inner_margin(Margin::same(10)),
                )
                .show(egui_ctx, |ui| {
                    ui.set_min_height(600.0);
                    // Zero the automatic widget spacing: the gutters below are
                    // explicit PANEL_GAP spacers only (round-13 issue 11).
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let mut gen_params = gen_params_state.lock().unwrap();

                    // 1. Header Bar
                    panels::header_bar::render_header(ui, &params, setter);
                    ui.add_space(PANEL_GAP);

                    // 2. Classic 303 Control Panel (includes integrated WAVE selector)
                    panels::top_controls::render_top_controls(ui, &params, setter);
                    ui.add_space(PANEL_GAP);

                    // 3. Souped-Up Engine Drawer Panel
                    panels::souped_panel::render_souped_panel(ui, &params, setter);
                    ui.add_space(PANEL_GAP);

                    // 4. Output Clipper + Tweaks Panel
                    // Clipper and tweaks stacked vertically to fill height and eliminate dead band
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        panels::output_clipper::render_output_clipper(ui, &params, setter);
                        ui.add_space(PANEL_GAP);
                        panels::tweaks_panel::render_tweaks_panel(ui, &params, setter);
                    });
                    ui.add_space(PANEL_GAP);

                    // 5. Bottom Sequencer Grid & Transport Column
                    panels::sequencer_panel::render_sequencer_and_transport(
                        ui,
                        &params,
                        &mut gen_params,
                        setter,
                        &current_playing_step,
                    );
                });
        },
    )
}
