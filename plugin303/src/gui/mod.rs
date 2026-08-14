pub mod panels;
pub mod theme;
pub mod widgets;

use nih_plug::prelude::Editor;
use nih_plug_egui::egui::{Frame, Margin};
use nih_plug_egui::{create_egui_editor, EguiState};
use std::sync::atomic::AtomicU8;
use std::sync::Arc;

use crate::params::Plugin303Params;

pub const GUI_WIDTH: u32 = 1080;
pub const GUI_HEIGHT: u32 = 640;

pub fn create_editor(
    params: Arc<Plugin303Params>,
    editor_state: Arc<EguiState>,
    current_playing_step: Arc<AtomicU8>,
) -> Option<Box<dyn Editor>> {
    create_egui_editor(
        editor_state,
        (),
        |_, _| {},
        move |egui_ctx, setter, _state| {
            egui_ctx.send_viewport_cmd(nih_plug_egui::egui::ViewportCommand::Title(
                "Vibe 303".into(),
            ));
            egui_ctx.set_visuals(nih_plug_egui::egui::Visuals::dark());
            nih_plug_egui::egui::CentralPanel::default()
                .frame(
                    Frame::new()
                        .fill(theme::CHASSIS_BG)
                        .inner_margin(Margin::same(8)),
                )
                .show(egui_ctx, |ui| {
                    // Header Bar
                    panels::header_bar::render_header(ui, &params, setter);
                    ui.add_space(6.0);

                    // Top Split: Classic Panel (Left) & Souped-Up Drawer (Right)
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = nih_plug_egui::egui::vec2(8.0, 8.0);
                        panels::classic_panel::render_classic(ui, &params, setter);
                        panels::souped_panel::render_souped(ui, &params, setter);
                    });

                    ui.add_space(8.0);

                    // Bottom: 16-Step Hardware Sequencer Console
                    panels::sequencer_panel::render_sequencer(
                        ui,
                        &params,
                        setter,
                        &current_playing_step,
                    );
                });
        },
    )
}
