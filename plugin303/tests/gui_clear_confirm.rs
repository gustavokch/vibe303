#![cfg(feature = "gui")]

//! Round-20 #9: CLEAR wipes all 16 steps, so one click only arms it. A second
//! click within 3 s clears; after that the tile reads CLEAR again and the next
//! click re-arms. Driven through the real `render_transport_panel` with
//! injected pointer events at set frame times.

mod gui_common;

use gui_common::{DummyGuiContext, Rig};
use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{vec2, Color32, Pos2, Rect, Ui};
use plugin303::gui::panels::sequencer_panel::{render_transport_panel, SEQUENCER_H};
use plugin303::gui::{split_columns, GUI_WIDTH, MARGIN};
use plugin303::params::{Plugin303Params, SequencerStep, StepGate};

/// Where CLEAR sits: the rightmost clickable tile of the bottom transport row.
fn clear_tile(rig: &Rig) -> Pos2 {
    let tiles: Vec<Rect> = rig
        .widgets()
        .iter()
        .filter(|w| w.sense.senses_click())
        .map(|w| w.rect)
        .collect();
    let bottom = tiles
        .iter()
        .map(|r| r.center().y)
        .fold(f32::NEG_INFINITY, f32::max);
    tiles
        .iter()
        .filter(|r| (r.center().y - bottom).abs() < 8.0)
        .max_by(|a, b| a.max.x.total_cmp(&b.max.x))
        .expect("the transport panel draws clickable tiles")
        .center()
}

/// The CLEAR tile's label in the painted text runs.
fn clear_label(texts: &[(String, Color32)]) -> &str {
    ["CONFIRM CLEAR", "CLEAR"]
        .into_iter()
        .find(|label| texts.iter().any(|(t, _)| t == label))
        .unwrap_or_else(|| panic!("no CLEAR tile in {texts:?}"))
}

/// A pattern CLEAR visibly changes: every step off the C3 NOTE default.
fn mark_pattern(params: &Plugin303Params) {
    let mut pattern = params.pattern.write().unwrap();
    for step in pattern.steps.iter_mut() {
        *step = SequencerStep {
            semitone: 7,
            octave: 1,
            gate: StepGate::Rest,
            accent: true,
            slide: true,
            gate_len: 0.9,
            velocity: 1.0,
        };
    }
}

fn is_marked(params: &Plugin303Params) -> bool {
    let pattern = params.pattern.read().unwrap();
    pattern
        .steps
        .iter()
        .all(|s| s.semitone == 7 && s.octave == 1 && s.gate == StepGate::Rest && s.accent)
}

fn is_cleared(params: &Plugin303Params) -> bool {
    let pattern = params.pattern.read().unwrap();
    pattern.steps.iter().all(|s| {
        s.semitone == 0
            && s.octave == 0
            && s.gate == StepGate::Note
            && !s.accent
            && !s.slide
            && s.gate_len == 0.5
            && s.velocity == 0.7
    })
}

/// Run `body` against a marked pattern and a rig showing the transport panel,
/// settled with the frame clock at 11 s.
fn with_transport(body: impl FnOnce(&Plugin303Params, &mut Rig, &mut dyn FnMut(&mut Ui))) {
    let params = Plugin303Params::default();
    mark_pattern(&params);
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);
    let (_, right_w) = split_columns(GUI_WIDTH as f32 - 2.0 * MARGIN);
    let mut rig = Rig::new(vec2(right_w, SEQUENCER_H));
    let mut add = |ui: &mut Ui| render_transport_panel(ui, &params, &setter);
    rig.set_time(10.0);
    let texts = rig.run(2, None, &mut add);
    assert_eq!(clear_label(&texts), "CLEAR", "CLEAR is unarmed at rest");
    body(&params, &mut rig, &mut add);
}

#[test]
fn first_click_arms_and_leaves_the_pattern() {
    with_transport(|params, rig, add| {
        let at = clear_tile(rig);
        let texts = rig.click(at, add);
        assert_eq!(clear_label(&texts), "CONFIRM CLEAR");
        assert!(is_marked(params), "a first click must not wipe the pattern");
    });
}

#[test]
fn second_click_within_the_window_clears() {
    with_transport(|params, rig, add| {
        let at = clear_tile(rig);
        rig.click(at, add); // arms at ~11 s
        assert!(is_marked(params));

        rig.set_time(13.0); // 2 s after the arm, inside the 3 s window
        let texts = rig.click(at, add);
        assert!(
            is_cleared(params),
            "the second click must clear all 16 steps"
        );
        assert_eq!(clear_label(&texts), "CLEAR", "clearing disarms the tile");
    });
}

#[test]
fn an_expired_arm_reads_clear_and_the_next_click_only_rearms() {
    with_transport(|params, rig, add| {
        let at = clear_tile(rig);
        rig.click(at, add); // arms at ~11 s

        rig.set_time(13.6); // this frame lands at 14.1 s, 3.1 s after the arm
        let texts = rig.run(1, None, add);
        assert_eq!(clear_label(&texts), "CLEAR", "the arm expires after 3 s");

        let texts = rig.click(at, add); // re-arms at ~14.1 s
        assert_eq!(
            clear_label(&texts),
            "CONFIRM CLEAR",
            "a click after expiry re-arms"
        );
        assert!(is_marked(params), "an expired arm must not clear");

        rig.set_time(15.0); // inside the new window
        rig.click(at, add);
        assert!(
            is_cleared(params),
            "the re-armed tile clears on its second click"
        );
    });
}
