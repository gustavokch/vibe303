#![cfg(feature = "gui")]

//! CALIBRATE is a lit action only while a rebuild is due. A trim that runs in
//! the background (after CALIBRATE, a topology switch or a project load) is
//! not a call to action: the tile stays raised and the status line alone says
//! CALIBRATING (round-22 #4).

mod gui_common;

use gui_common::{filled_rects, text_runs, DummyGuiContext, EnvGuard, Rig, ENV_LOCK};
use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{vec2, Ui};
use plugin303::gui::panels::tweaks_panel;
use plugin303::gui::{split_columns, theme, GUI_WIDTH, MARGIN, TWEAKS_H};
use plugin303::params::{Plugin303Params, VcoTrim};
use std::sync::atomic::Ordering;

/// Whether the CALIBRATE tile is lit, and the status line, for `params`.
fn calibrate_state(params: &Plugin303Params) -> (bool, String) {
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);
    let width = split_columns(GUI_WIDTH as f32 - 2.0 * MARGIN).1;
    let mut rig = Rig::new(vec2(width, TWEAKS_H));
    let shapes =
        rig.shapes(&mut |ui: &mut Ui| tweaks_panel::render_tweaks_panel(ui, params, &setter));
    let texts = text_runs(&shapes);
    let tile = texts
        .iter()
        .find(|(t, _)| t == "CALIBRATE")
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("no CALIBRATE tile in {texts:?}"));
    let lit = filled_rects(&shapes, theme::ACCENT_ORANGE)
        .iter()
        .any(|r| r.contains_rect(tile));
    let status = ["CALIBRATION DUE", "CALIBRATING", "CALIBRATED"]
        .into_iter()
        .find(|s| texts.iter().any(|(t, _)| t == s))
        .unwrap_or_else(|| panic!("no status line in {texts:?}"))
        .to_string();
    (lit, status)
}

#[test]
fn calibrate_lights_only_when_a_rebuild_is_due() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvGuard::set("VIBE303_QA_STATE", "default");

    let params = Plugin303Params::default();
    assert_eq!(
        calibrate_state(&params),
        (false, "CALIBRATED".to_string()),
        "at rest"
    );

    params.vco_trim_busy.store(true, Ordering::Relaxed);
    assert_eq!(
        calibrate_state(&params),
        (false, "CALIBRATING".to_string()),
        "a trim runs and none is due"
    );

    // The parameters ask for another trim than the stored one.
    *params.vco_trim.write().unwrap() = VcoTrim {
        age: 5,
        ..VcoTrim::NEUTRAL
    };
    assert_eq!(
        calibrate_state(&params),
        (true, "CALIBRATION DUE".to_string()),
        "a rebuild is due while a trim runs"
    );

    params.vco_trim_busy.store(false, Ordering::Relaxed);
    assert_eq!(
        calibrate_state(&params),
        (true, "CALIBRATION DUE".to_string()),
        "a rebuild is due"
    );
}
