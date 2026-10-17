#![cfg(feature = "gui")]

use nih_plug::prelude::{GuiContext, PluginApi};
use nih_plug_egui::egui::{self, Frame, Margin};
use plugin303::gui::panels::sequencer_panel::{render_transport_panel, SEQUENCER_H};
use plugin303::gui::widgets::DevicePanel;
use plugin303::gui::{render_editor, split_columns, theme, GUI_HEIGHT, GUI_WIDTH, MARGIN};
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

/// Content that overflows a device card must reach the parent's measured
/// rect, or the editor test above cannot see a clipped panel.
#[test]
fn device_panel_overflow_reaches_parent_rect() {
    let ctx = egui::Context::default();
    let raw_input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(400.0, 400.0),
        )),
        ..Default::default()
    };
    let card_h = 60.0;
    let mut measured = egui::Rect::NOTHING;
    let _ = ctx.run(raw_input, |ctx| {
        egui::CentralPanel::default()
            .frame(Frame::new())
            .show(ctx, |ui| {
                let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(ui.max_rect()));
                DevicePanel::new("OVERFLOW", card_h).show(
                    &mut inner,
                    |_| {},
                    |body| {
                        body.allocate_exact_size(
                            egui::vec2(10.0, 3.0 * card_h),
                            egui::Sense::hover(),
                        );
                    },
                );
                measured = inner.min_rect();
            });
    });
    assert!(
        measured.height() > card_h,
        "body overflow hidden from the parent: measured {} vs card {}",
        measured.height(),
        card_h
    );
}

/// Transport rows are two equal slots of same-height tiles: every row's left
/// slot ends on Row 1's seam, every row reaches Row 1's right edge, and every
/// tile is Row 1's height.
#[test]
fn transport_rows_share_one_seam_and_tile_height() {
    let params = Arc::new(Plugin303Params::default());
    let dummy_ctx = DummyGuiContext;
    let setter = nih_plug::prelude::ParamSetter::new(&dummy_ctx);
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let (_, right_w) = split_columns(GUI_WIDTH as f32 - 2.0 * MARGIN);
    let raw_input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(right_w, SEQUENCER_H),
        )),
        ..Default::default()
    };
    for _ in 0..2 {
        let _ = ctx.run(raw_input.clone(), |ctx| {
            egui::CentralPanel::default()
                .frame(Frame::new())
                .show(ctx, |ui| render_transport_panel(ui, &params, &setter));
        });
    }

    // Tiles: clickable widgets at least one interact row tall. Labels are
    // shorter, and the card itself only senses hover.
    let min_h = ctx.style().spacing.interact_size.y - 0.5;
    let mut tiles: Vec<egui::Rect> = ctx.viewport(|vp| {
        vp.prev_pass
            .widgets
            .layers()
            .flat_map(|(_, ws)| ws.iter())
            .filter(|w| w.sense.senses_click() && w.rect.height() >= min_h)
            .map(|w| w.rect)
            .collect()
    });
    tiles.sort_by(|a, b| a.center().y.total_cmp(&b.center().y));
    let mut rows: Vec<Vec<egui::Rect>> = Vec::new();
    for t in tiles {
        match rows.last_mut() {
            Some(row) if (row[0].center().y - t.center().y).abs() < 8.0 => row.push(t),
            _ => rows.push(vec![t]),
        }
    }
    assert_eq!(rows.len(), 6, "six transport rows expected: {rows:?}");

    let tile_h = rows[0][0].height();
    let seam = rows[0]
        .iter()
        .map(|r| r.max.x)
        .fold(f32::INFINITY, f32::min);
    let right = rows[0]
        .iter()
        .map(|r| r.max.x)
        .fold(f32::NEG_INFINITY, f32::max);
    for (i, row) in rows.iter().enumerate() {
        for t in row {
            assert!(
                (t.height() - tile_h).abs() < 0.5,
                "row {}: tile {t:?} is not {tile_h} tall",
                i + 1
            );
        }
        assert!(
            row.iter().any(|t| (t.max.x - seam).abs() < 1.0),
            "row {}: no tile ends on the seam x={seam}: {row:?}",
            i + 1
        );
        let row_right = row
            .iter()
            .map(|r| r.max.x)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (row_right - right).abs() < 1.0,
            "row {}: ends at {row_right}, card edge is {right}",
            i + 1
        );
    }
}
