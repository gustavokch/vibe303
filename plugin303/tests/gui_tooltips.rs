#![cfg(feature = "gui")]

mod gui_common;

use gui_common::{DummyGuiContext, EnvGuard, Rig, ENV_LOCK};
use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{self, vec2, Rect, Ui};
use plugin303::gui::panels::{header_bar, top_controls, tweaks_panel};
use plugin303::gui::widgets::toggle::TOGGLE_H;
use plugin303::gui::{split_columns, GUI_WIDTH, HEADER_BAR_H, MARGIN, TWEAKS_H, VOICE_ROW_H};
use plugin303::params::Plugin303Params;

/// Room below a device for its tooltip.
const TOOLTIP_ROOM: f32 = 200.0;

/// A control: something a user clicks or drags, or a read-only value well.
fn is_control(w: &egui::WidgetRect) -> bool {
    let readout = (w.rect.width() - tweaks_panel::FIELD_W).abs() < 0.5
        && (w.rect.height() - TOGGLE_H).abs() < 0.5;
    w.sense.senses_click() || w.sense.senses_drag() || readout
}

/// Every disabled control the device draws explains, on hover, why it is
/// disabled. egui shows `on_hover_text` only on enabled widgets.
fn assert_disabled_controls_explain(
    rig: &mut Rig,
    add: &mut dyn FnMut(&mut Ui),
    hint: &str,
    at_least: usize,
) {
    rig.run(2, None, add);
    let targets: Vec<Rect> = rig
        .widgets()
        .into_iter()
        .filter(|w| !w.enabled && is_control(w))
        .map(|w| w.rect)
        .collect();
    assert!(
        targets.len() >= at_least,
        "expected at least {at_least} disabled controls, found {targets:?}"
    );
    for rect in targets {
        let texts = rig.hover(rect, add);
        assert!(
            texts.iter().any(|t| t == hint),
            "disabled control at {rect:?} does not show {hint:?}; on screen: {texts:?}"
        );
    }
}

fn right_column_width() -> f32 {
    split_columns(GUI_WIDTH as f32 - 2.0 * MARGIN).1
}

#[test]
fn accurate_tweaks_explain_why_they_are_inert() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvGuard::set("VIBE303_QA_STATE", "accurate");
    let params = Plugin303Params::default();
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);
    let mut rig = Rig::new(vec2(right_column_width(), TWEAKS_H + TOOLTIP_ROOM));
    // 4 Readouts + DRIFT + RECAL.
    assert_disabled_controls_explain(
        &mut rig,
        &mut |ui| tweaks_panel::render_tweaks_panel(ui, &params, &setter),
        tweaks_panel::ACCURATE_HINT,
        6,
    );
}

#[test]
fn accurate_quality_explains_why_it_is_inert() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvGuard::set("VIBE303_QA_STATE", "accurate");
    let params = Plugin303Params::default();
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);
    let width = GUI_WIDTH as f32 - 2.0 * MARGIN;
    let mut rig = Rig::new(vec2(width, HEADER_BAR_H + TOOLTIP_ROOM));
    assert_disabled_controls_explain(
        &mut rig,
        &mut |ui| header_bar::render_header(ui, &params, &setter),
        header_bar::QUALITY_ACCURATE_HINT,
        1,
    );
}

#[test]
fn envelope_banks_without_adv_env_say_how_to_enable_them() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let params = Plugin303Params::default(); // FUN, ADV ENV off
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);
    let mut rig = Rig::new(vec2(right_column_width(), VOICE_ROW_H + TOOLTIP_ROOM));
    // MEG A D S R + VEG A D S R: the voice reads neither bank without ADV ENV.
    assert_disabled_controls_explain(
        &mut rig,
        &mut |ui| top_controls::render_envelope_panel(ui, &params, &setter),
        top_controls::ADV_ENV_OFF_HINT,
        8,
    );
}

#[test]
fn accurate_envelopes_explain_that_adv_env_is_fun_only() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvGuard::set("VIBE303_QA_STATE", "accurate");
    let params = Plugin303Params::default();
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);
    let mut rig = Rig::new(vec2(right_column_width(), VOICE_ROW_H + TOOLTIP_ROOM));
    // 8 faders + the ADV ENV tile.
    assert_disabled_controls_explain(
        &mut rig,
        &mut |ui| top_controls::render_envelope_panel(ui, &params, &setter),
        top_controls::ADV_ENV_ACCURATE_HINT,
        9,
    );
}
