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

pub fn create_editor(
    params: Arc<Plugin303Params>,
    editor_state: Arc<EguiState>,
    current_playing_step: Arc<AtomicU8>,
) -> Option<Box<dyn Editor>> {
    let gen_params_state = Arc::new(std::sync::Mutex::new(
        crate::dsp::generator::GeneratorParams::default(),
    ));

    create_egui_editor(
        editor_state,
        (),
        |_, _| {},
        move |egui_ctx, setter, _state| {
            egui_ctx.send_viewport_cmd(nih_plug_egui::egui::ViewportCommand::Title(
                "TECHNEO 303".into(),
            ));
            egui_ctx.set_visuals(nih_plug_egui::egui::Visuals::dark());
            nih_plug_egui::egui::CentralPanel::default()
                .frame(
                    Frame::new()
                        .fill(theme::CHASSIS_BG)
                        .inner_margin(Margin::same(10)),
                )
                .show(egui_ctx, |ui| {
                    ui.set_min_height(600.0);
                    let mut gen_params = gen_params_state.lock().unwrap();

                    // 1. Header Bar
                    panels::header_bar::render_header(ui, &params, setter);
                    ui.add_space(0.5);

                    // 2. Classic 303 Control Panel (includes integrated WAVE selector)
                    panels::top_controls::render_top_controls(ui, &params, setter);
                    ui.add_space(0.5);

                    // 3. Souped-Up Engine Drawer Panel
                    panels::souped_panel::render_souped_panel(ui, &params, setter);
                    ui.add_space(0.5);

                    // 4. Output Clipper + Tweaks Panel
                    ui.horizontal(|ui| {
                        // Clipper and tweaks stacked vertically to fill height and eliminate dead band
                        ui.vertical(|ui| {
                            panels::output_clipper::render_output_clipper(ui, &params, setter);
                            ui.add_space(3.0);
                            panels::tweaks_panel::render_tweaks_panel(ui, &params, setter);
                        });
                    });
                    ui.add_space(0.5);

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
