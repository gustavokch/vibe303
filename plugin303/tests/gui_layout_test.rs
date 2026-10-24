#![cfg(feature = "gui")]

mod gui_common;
use gui_common::{filled_rects, text_runs, DummyGuiContext, EnvGuard, Rig, ENV_LOCK};

use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{self, vec2, Frame, Margin, Rect, Ui, Vec2};
use plugin303::gui::panels::sequencer_panel::{render_transport_panel, SEQUENCER_H};
use plugin303::gui::panels::{output_clipper, souped_panel, top_controls};
use plugin303::gui::widgets::DevicePanel;
use plugin303::gui::{
    render_editor, split_columns, theme, BANK_ROW_H, GUI_HEIGHT, GUI_WIDTH, MARGIN, VOICE_ROW_H,
};
use plugin303::params::Plugin303Params;
use std::sync::atomic::AtomicU8;
use std::sync::Arc;

/// The full editor, rendered through the same `render_editor` the plugin uses,
/// must fill the window: no horizontal or vertical overflow, and no dead band
/// below the last device.
#[test]
fn editor_fills_window_without_overflow_or_dead_band() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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

/// Each knob value pill of a device, with its value, the value's text rect and
/// the footprint of the knob that paints it. A pill is a `WELL_BG` rect of the
/// pill height; its knob is the dragged widget under its centre.
fn knob_pills(rig: &mut Rig, add: &mut dyn FnMut(&mut Ui)) -> Vec<(String, Rect, Rect, Rect)> {
    let shapes = rig.shapes(add);
    let texts = text_runs(&shapes);
    let knobs: Vec<Rect> = rig
        .widgets()
        .into_iter()
        .filter(|w| w.sense.senses_drag())
        .map(|w| w.rect)
        .collect();
    filled_rects(&shapes, theme::WELL_BG)
        .into_iter()
        .filter(|r| (r.height() - theme::KNOB_PILL_HEIGHT).abs() < 0.01)
        .map(|pill| {
            let (value, text) = texts
                .iter()
                .find(|(_, r)| pill.contains(r.center()))
                .cloned()
                .unwrap_or_else(|| panic!("value pill at {pill:?} holds no text"));
            let knob = *knobs
                .iter()
                .find(|k| k.contains(pill.center()))
                .unwrap_or_else(|| panic!("the pill of {value:?} at {pill:?} has no knob"));
            (value, text, pill, knob)
        })
        .collect()
}

/// The device that `add` draws in `size` has `count` knobs. Every knob value
/// fits its pill with at least 1 pt on each side, and every pill stays inside
/// its knob's footprint.
fn assert_knob_values_fit(device: &str, size: Vec2, count: usize, add: &mut dyn FnMut(&mut Ui)) {
    let knobs = knob_pills(&mut Rig::new(size), add);
    assert_eq!(
        knobs.len(),
        count,
        "{device}: expected {count} knobs, found {knobs:?}"
    );
    for (value, text, pill, knob) in knobs {
        assert!(
            text.left() >= pill.left() + 1.0 && text.right() <= pill.right() - 1.0,
            "{device}: {value:?} is {:.1} pt wide in a {:.1} pt pill",
            text.width(),
            pill.width()
        );
        assert!(
            knob.contains_rect(pill),
            "{device}: the pill of {value:?} ({pill:?}) leaves its knob ({knob:?})"
        );
    }
}

/// At the widest values the knobs reach (`knob_extremes`: PITCH ±24 st, IN/OUT
/// ±18 dB, TUNING -100 ct), every value fits its pill and every pill fits its
/// knob. Round 21 #2: 8-character values were 39.7 pt in a 38 pt pill.
#[test]
fn knob_values_fit_their_pills_and_pills_fit_their_knobs() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvGuard::set("VIBE303_QA_STATE", "knob_extremes");
    let params = Plugin303Params::default();
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);
    let left = split_columns(GUI_WIDTH as f32 - 2.0 * MARGIN).0;

    assert_knob_values_fit("VOICE", vec2(left, VOICE_ROW_H), 7, &mut |ui| {
        top_controls::render_voice_panel(ui, &params, &setter)
    });
    assert_knob_values_fit("SOUPED-UP ENGINE", vec2(left, BANK_ROW_H), 9, &mut |ui| {
        souped_panel::render_souped_panel(ui, &params, &setter)
    });
    assert_knob_values_fit("OUTPUT CLIPPER", vec2(left, BANK_ROW_H), 9, &mut |ui| {
        output_clipper::render_output_clipper(ui, &params, &setter)
    });
}
