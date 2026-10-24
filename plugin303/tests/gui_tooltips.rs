#![cfg(feature = "gui")]

mod gui_common;

use gui_common::{DummyGuiContext, EnvGuard, Rig, ENV_LOCK};
use nih_plug::prelude::{BoolParam, ParamSetter};
use nih_plug_egui::egui::{self, vec2, Rect, Ui};
use plugin303::dsp::generator::GeneratorParams;
use plugin303::gui::panels::generator_panel::{self, GENERATOR_H};
use plugin303::gui::panels::sequencer_panel::SEQUENCER_H;
use plugin303::gui::panels::{
    header_bar, output_clipper, sequencer_panel, souped_panel, top_controls, tweaks_panel,
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

/// In ACCURATE the SOUPED-UP ENGINE body is a banner with one action; it says
/// what it does on hover like every other engine control (round-20 #11).
#[test]
fn accurate_banner_action_explains_itself() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvGuard::set("VIBE303_QA_STATE", "accurate");
    let params = Plugin303Params::default();
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);

    let mut rig = Rig::new(vec2(left_column_width(), BANK_ROW_H + TOOLTIP_ROOM));
    let controls = tooltips_by_control(&mut rig, &mut |ui| {
        souped_panel::render_souped_panel(ui, &params, &setter)
    });
    // The header switches are hidden. The banner's two text labels sense clicks
    // (text selection); the action is the one tile of toggle height.
    let actions: Vec<_> = controls
        .iter()
        .filter(|(rect, _)| (rect.height() - TOGGLE_H).abs() < 0.5)
        .collect();
    assert_eq!(
        actions.len(),
        1,
        "expected one action tile, found {controls:?}"
    );
    let (rect, texts) = actions[0];
    assert!(
        texts.iter().any(|t| t == souped_panel::ENABLE_FUN_HINT),
        "ENABLE FUN MODE at {rect:?} does not show {:?}; on screen: {texts:?}",
        souped_panel::ENABLE_FUN_HINT
    );
}

/// The generator's widgets that sense a click or drag, in layout order (each
/// column top to bottom), and the tooltip the contract demands of each. The
/// group headings are here because egui's selectable labels sense clicks.
/// `None`: a plain word, no abbreviation to explain.
const GENERATOR_CONTROLS: [(&str, Option<&str>); 18] = [
    ("PATTERN STRUCTURE heading", None),
    ("ROOT", None),
    ("SCALE", None),
    (
        "PULSES (k)",
        Some("Euclidean pulses: notes spread evenly over STEPS"),
    ),
    ("STEPS (n)", Some("Euclidean pattern length in steps")),
    ("SHIFT", None),
    (
        "PITCH LEN",
        Some(
            "Pitch sequence length. When it differs from STEPS, pitches and rhythm repeat at \
             different lengths.",
        ),
    ),
    ("OCTAVE BEHAVIOR heading", None),
    (
        "OCT RANGE MIN",
        Some("Lowest octave a generated note may use."),
    ),
    (
        "OCT RANGE MAX",
        Some("Highest octave a generated note may use."),
    ),
    (
        "OCT UP",
        Some("Chance that a note steps one octave up from the previous note."),
    ),
    (
        "OCT DN",
        Some("Chance that a note steps one octave down from the previous note."),
    ),
    ("PROBABILITIES heading", None),
    ("REST", None),
    ("ACCENT", None),
    ("SLIDE", None),
    (
        "ACC BIAS",
        Some("Chance that an accented note moves to the upper half of the scale"),
    ),
    (
        "SLD CHAIN",
        Some("Lit: slides may follow slides. Unlit: never two slides in a row."),
    ),
];

/// The generator abbreviates its fields (PULSES (k), STEPS (n), PITCH LEN,
/// OCT RANGE MIN/MAX, OCT UP, OCT DN, ACC BIAS, SLD CHAIN); each control, and
/// no other, shows its own text on hover (round-20 #11).
#[test]
fn generator_abbreviated_fields_explain_themselves() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvGuard::set("VIBE303_QA_STATE", "default");
    let mut gen_params = GeneratorParams::default();
    let mut rig = Rig::new(vec2(left_column_width(), GENERATOR_H + TOOLTIP_ROOM));
    let controls = tooltips_by_control(&mut rig, &mut |ui| {
        generator_panel::render_generator_panel(ui, &mut gen_params)
    });
    assert_eq!(
        controls.len(),
        GENERATOR_CONTROLS.len(),
        "the generator's controls changed; update GENERATOR_CONTROLS: {controls:?}"
    );
    for ((rect, texts), (name, expected)) in controls.iter().zip(GENERATOR_CONTROLS) {
        if let Some(expected) = expected {
            assert_eq!(
                texts.as_slice(),
                [expected],
                "{name} at {rect:?} shows the wrong tooltip"
            );
        }
    }
}

/// GENERATION controls: MUTATION, NEW SEED, GENERATE, MUTATE.
const GENERATION_CONTROLS: [(&str, Option<&str>); 4] = [
    ("MUTATION", None),
    ("NEW SEED", None),
    ("GENERATE", None),
    ("MUTATE", None),
];

#[test]
fn generation_controls_explain_themselves() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvGuard::set("VIBE303_QA_STATE", "default");
    let params = Plugin303Params::default();
    let mut gen_params = GeneratorParams::default();
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);
    let mut rig = Rig::new(vec2(right_column_width(), GENERATOR_H + TOOLTIP_ROOM));
    let controls = tooltips_by_control(&mut rig, &mut |ui| {
        generator_panel::render_generation_panel(ui, &params, &mut gen_params, &setter);
    });
    assert_eq!(
        controls.len(),
        GENERATION_CONTROLS.len(),
        "the generation panel's controls changed; update GENERATION_CONTROLS: {controls:?}"
    );
    for ((rect, texts), (name, expected)) in controls.iter().zip(GENERATION_CONTROLS) {
        if let Some(expected) = expected {
            assert_eq!(
                texts.as_slice(),
                [expected],
                "{name} at {rect:?} shows the wrong tooltip"
            );
        }
    }
}

/// A bypassed OUTPUT CLIPPER greys its switches and knobs; each says how to
/// wake them (`on_hover_text` never shows on a disabled widget).
#[test]
fn bypassed_clipper_explains_why_it_is_inert() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvGuard::set("VIBE303_QA_STATE", "default");
    let params = Plugin303Params {
        clipper_bypass: BoolParam::new("Clipper Bypass", true),
        ..Default::default()
    };
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);
    let mut rig = Rig::new(vec2(left_column_width(), BANK_ROW_H + TOOLTIP_ROOM));
    // OS + DIODE + LINK + 9 knobs; ON stays live.
    assert_disabled_controls_explain(
        &mut rig,
        &mut |ui| output_clipper::render_output_clipper(ui, &params, &setter),
        output_clipper::CLIPPER_OFF_HINT,
        12,
    );
}

/// The two TRANSPORT combos (RATE/timebase and MODE/sequencer mode) explain
/// what both options do on hover (round-21 #1).
#[test]
fn transport_combos_explain_themselves() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvGuard::set("VIBE303_QA_STATE", "default");
    let params = Plugin303Params::default();
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);
    let mut rig = Rig::new(vec2(right_column_width(), SEQUENCER_H + TOOLTIP_ROOM));

    let controls = tooltips_by_control(&mut rig, &mut |ui| {
        sequencer_panel::render_transport_panel(ui, &params, &setter)
    });

    let tb_tooltip = "Timebase: 16th is straight 16th steps. 12th is triplet 12th steps.";
    let mode_tooltip = "Mode: Direct plays the pitch stored on that step. Decoupled is the TB-303 pitch/time split: NOTE steps take stored pitches in order, so a TIE or REST does not use a pitch.";

    let has_tb = controls
        .iter()
        .any(|(_, texts)| texts.iter().any(|t| t == tb_tooltip));
    assert!(
        has_tb,
        "RATE combo does not show tooltip {tb_tooltip:?}; controls: {controls:?}"
    );

    let has_mode = controls
        .iter()
        .any(|(_, texts)| texts.iter().any(|t| t == mode_tooltip));
    assert!(
        has_mode,
        "MODE combo does not show tooltip {mode_tooltip:?}; controls: {controls:?}"
    );
}
