#![cfg(feature = "gui")]

use nih_plug::prelude::{GuiContext, PluginApi};
use nih_plug_egui::egui::{self, Frame, Margin};
use plugin303::gui::{render_editor, theme, GUI_HEIGHT, GUI_WIDTH, MARGIN};
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
    unsafe fn raw_set_parameter_normalized(
        &self,
        _param: nih_plug::prelude::ParamPtr,
        _normalized: f32,
    ) {
    }
    unsafe fn raw_end_set_parameter(&self, _param: nih_plug::prelude::ParamPtr) {}
    fn get_state(&self) -> nih_plug::prelude::PluginState {
        unimplemented!("state query not exercised by layout tests")
    }
    fn set_state(&self, _state: nih_plug::prelude::PluginState) {}
}

/// The full editor, rendered through the same `render_editor` the plugin uses,
/// must fill the window: no horizontal or vertical overflow, and no dead band
/// below the last device.
#[test]
fn editor_fills_window_without_overflow_or_dead_band() {
    let params = Arc::new(Plugin303Params::default());
    let current_playing_step = Arc::new(AtomicU8::new(0));
    let mut gen_params = plugin303::dsp::generator::GeneratorParams::default();

    let dummy_ctx = DummyGuiContext;
    let setter = nih_plug::prelude::ParamSetter::new(&dummy_ctx);

    let ctx = egui::Context::default();
    theme::apply(&ctx);

    let raw_input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(GUI_WIDTH as f32, GUI_HEIGHT as f32),
        )),
        ..Default::default()
    };

    let mut content = egui::Rect::NOTHING;
    // egui lays a fresh Ui out over two passes; check the settled one.
    for _ in 0..2 {
        let _ = ctx.run(raw_input.clone(), |ctx| {
            egui::CentralPanel::default()
                .frame(
                    Frame::new()
                        .fill(theme::CHASSIS_BG)
                        .inner_margin(Margin::same(MARGIN as i8)),
                )
                .show(ctx, |ui| {
                    // CentralPanel stretches its own Ui to the whole window, so
                    // measure in a child that only grows with real content.
                    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(ui.max_rect()));
                    render_editor(
                        &mut inner,
                        &params,
                        &setter,
                        &mut gen_params,
                        &current_playing_step,
                    );
                    content = inner.min_rect();
                });
        });
    }

    let right_limit = GUI_WIDTH as f32 - MARGIN + 0.5;
    let bottom_limit = GUI_HEIGHT as f32 - MARGIN + 0.5;
    assert!(
        content.max.x <= right_limit,
        "content overflows the window width: max.x = {}",
        content.max.x
    );
    assert!(
        content.max.x >= GUI_WIDTH as f32 - MARGIN - 0.5,
        "devices stop short of the right margin: max.x = {}",
        content.max.x
    );
    assert!(
        content.max.y <= bottom_limit,
        "content overflows the window height: max.y = {}",
        content.max.y
    );
    assert!(
        content.max.y >= bottom_limit - 8.0,
        "dead band below the last device: max.y = {}, window bottom = {}",
        content.max.y,
        bottom_limit
    );
}

/// Product requirement: the window is exactly 19:10.
#[test]
fn window_aspect_ratio_is_19_to_10() {
    assert_eq!(GUI_WIDTH * 10, GUI_HEIGHT * 19);
}
