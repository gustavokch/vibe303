#![cfg(feature = "gui")]

mod gui_common;

use gui_common::{DummyGuiContext, EnvGuard, Rig, ENV_LOCK};
use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{self, vec2, Rect, Ui};
use plugin303::dsp::generator::GeneratorParams;
use plugin303::gui::panels::generator_panel::{self, GENERATOR_H};
use plugin303::gui::panels::{
    header_bar, output_clipper, souped_panel, top_controls, tweaks_panel,
};
use plugin303::gui::widgets::toggle::TOGGLE_H;
use plugin303::gui::{
    split_columns, BANK_ROW_H, GUI_WIDTH, HEADER_BAR_H, MARGIN, TWEAKS_H, VOICE_ROW_H,
};
use plugin303::params::Plugin303Params;
use std::collections::HashSet;

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
    let _env = EnvGuard::set("VIBE303_QA_STATE", "default");
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

/// For each enabled control the device draws: its rect and the text that
/// appears on screen only while the pointer rests on it.
fn tooltips_by_control(rig: &mut Rig, add: &mut dyn FnMut(&mut Ui)) -> Vec<(Rect, Vec<String>)> {
    let baseline: HashSet<String> = rig.run(2, None, add).into_iter().map(|(t, _)| t).collect();
    let targets: Vec<Rect> = rig
        .widgets()
        .into_iter()
        .filter(|w| w.enabled && (w.sense.senses_click() || w.sense.senses_drag()))
        .map(|w| w.rect)
        .collect();
    targets
        .into_iter()
        .map(|rect| {
            let texts = rig
                .hover(rect, add)
                .into_iter()
                .filter(|t| !baseline.contains(t))
                .collect();
            (rect, texts)
        })
        .collect()
}

/// Every enabled control the device draws shows a tooltip on hover.
fn assert_controls_explain(rig: &mut Rig, add: &mut dyn FnMut(&mut Ui), at_least: usize) {
    let controls = tooltips_by_control(rig, add);
    assert!(
        controls.len() >= at_least,
        "expected at least {at_least} controls, found {}: {controls:?}",
        controls.len()
    );
    for (rect, texts) in controls {
        assert!(!texts.is_empty(), "control at {rect:?} shows no tooltip");
    }
}

fn left_column_width() -> f32 {
    split_columns(GUI_WIDTH as f32 - 2.0 * MARGIN).0
}

/// The engine devices label their controls with abbreviations (MIX2, R-COMP,
/// DYN, …); each control says what it is on hover (round-20 #11).
#[test]
fn engine_device_controls_explain_themselves() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvGuard::set("VIBE303_QA_STATE", "default");
    let params = Plugin303Params::default(); // FUN, clipper on
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);

    let mut rig = Rig::new(vec2(left_column_width(), BANK_ROW_H + TOOLTIP_ROOM));
    // 9 knobs + VCA ENV + VCF TAPS.
    assert_controls_explain(
        &mut rig,
        &mut |ui| souped_panel::render_souped_panel(ui, &params, &setter),
        11,
    );

    let mut rig = Rig::new(vec2(left_column_width(), BANK_ROW_H + TOOLTIP_ROOM));
    // 9 knobs + ON + OS + DIODE + LINK.
    assert_controls_explain(
        &mut rig,
        &mut |ui| output_clipper::render_output_clipper(ui, &params, &setter),
        13,
    );

    let mut rig = Rig::new(vec2(right_column_width(), TWEAKS_H + TOOLTIP_ROOM));
    // CALIBRATE + TOL + AGE + NOISE + CMP SEED + DRIFT + RECAL.
    assert_controls_explain(
        &mut rig,
        &mut |ui| tweaks_panel::render_tweaks_panel(ui, &params, &setter),
        7,
    );
}

/// The generator spells its fields out as abbreviations (PULSES (k), STEPS (n),
/// ACC BIAS, SLD CHAIN); each says what it is on hover (round-20 #11).
#[test]
fn generator_abbreviated_fields_explain_themselves() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvGuard::set("VIBE303_QA_STATE", "default");
    let mut gen_params = GeneratorParams::default();
    let mut rig = Rig::new(vec2(left_column_width(), GENERATOR_H + TOOLTIP_ROOM));
    let shown: HashSet<String> = tooltips_by_control(&mut rig, &mut |ui| {
        generator_panel::render_generator_panel(ui, &mut gen_params)
    })
    .into_iter()
    .flat_map(|(_, texts)| texts)
    .collect();
    for expected in [
        "Euclidean pulses: notes spread evenly over STEPS",
        "Euclidean pattern length in steps",
        "Chance that an accented note moves to the upper half of the scale",
        "Lit: slides may follow slides. Unlit: never two slides in a row.",
    ] {
        assert!(shown.contains(expected), "no control shows {expected:?}");
    }
}
