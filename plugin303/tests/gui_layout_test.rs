use nih_plug::prelude::{GuiContext, PluginApi};
use nih_plug_egui::egui::{self, Frame, Margin};
use plugin303::gui::{panels, theme, GUI_HEIGHT, GUI_WIDTH};
use plugin303::params::Plugin303Params;
use std::sync::atomic::AtomicU8;
use std::sync::Arc;

struct DummyGuiContext;
impl GuiContext for DummyGuiContext {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Standalone
    }
    fn request_resize(&self) -> bool {
        false
    }
    unsafe fn raw_begin_set_parameter(&self, _param: nih_plug::prelude::ParamPtr) {}
    unsafe fn raw_set_parameter_normalized(&self, _param: nih_plug::prelude::ParamPtr, _normalized: f32) {}
    unsafe fn raw_end_set_parameter(&self, _param: nih_plug::prelude::ParamPtr) {}
    fn get_state(&self) -> nih_plug::prelude::PluginState {
        unimplemented!()
    }
    fn set_state(&self, _state: nih_plug::prelude::PluginState) {}
}

#[test]
fn test_all_panels_sequential_full_ui() {
    let params = Arc::new(Plugin303Params::default());
    let current_playing_step = Arc::new(AtomicU8::new(0));
    let gen_params_state = Arc::new(std::sync::Mutex::new(
        plugin303::dsp::generator::GeneratorParams::default(),
    ));

    let dummy_ctx = DummyGuiContext;
    let setter = nih_plug::prelude::ParamSetter::new(&dummy_ctx);

    let ctx = egui::Context::default();
    ctx.set_visuals(egui::Visuals::dark());

    let raw_input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(GUI_WIDTH as f32, GUI_HEIGHT as f32),
        )),
        ..Default::default()
    };

    let _output = ctx.run(raw_input, |ctx| {
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(theme::CHASSIS_BG)
                    .inner_margin(Margin::same(10)),
            )
            .show(ctx, |ui| {
                ui.set_min_height(600.0);
                let mut gen_params = gen_params_state.lock().unwrap();

                // 1. Header Bar
                panels::header_bar::render_header(ui, &params, &setter);
                assert!(ui.min_rect().max.x <= 1356.5, "Header overflow: max.x = {}", ui.min_rect().max.x);
                ui.add_space(0.5);

                // 2. Classic 303 Control Panel
                panels::top_controls::render_top_controls(ui, &params, &setter);
                assert!(ui.min_rect().max.x <= 1356.5, "Top controls overflow: max.x = {}", ui.min_rect().max.x);
                ui.add_space(0.5);

                // 3. Souped-Up Engine Drawer Panel
                panels::souped_panel::render_souped_panel(ui, &params, &setter);
                assert!(ui.min_rect().max.x <= 1356.5, "Souped panel overflow: max.x = {}", ui.min_rect().max.x);
                ui.add_space(0.5);

                // 4. Output Clipper + Tweaks Panel
                ui.vertical(|ui| {
                    panels::output_clipper::render_output_clipper(ui, &params, &setter);
                    assert!(ui.min_rect().max.x <= 1356.5, "Clipper overflow: max.x = {}", ui.min_rect().max.x);
                    ui.add_space(3.0);
                    panels::tweaks_panel::render_tweaks_panel(ui, &params, &setter);
                    assert!(ui.min_rect().max.x <= 1356.5, "Tweaks overflow: max.x = {}", ui.min_rect().max.x);
                });
                assert!(ui.min_rect().max.x <= 1356.5, "Post-clipper/tweaks overflow: max.x = {}", ui.min_rect().max.x);
                ui.add_space(0.5);

                // 5. Bottom Sequencer Grid & Transport Column
                panels::sequencer_panel::render_sequencer_and_transport(
                    ui,
                    &params,
                    &mut gen_params,
                    &setter,
                    &current_playing_step,
                );
                assert!(ui.min_rect().max.x <= 1356.5, "Sequencer overflow: max.x = {}", ui.min_rect().max.x);
            });
    });
}
