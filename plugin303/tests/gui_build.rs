#[cfg(feature = "gui")]
#[test]
fn test_gui_initialization_build() {
    use plugin303::params::Plugin303Params;
    use std::sync::atomic::AtomicU8;
    use std::sync::Arc;

    let params = Arc::new(Plugin303Params::default());
    let state =
        nih_plug_egui::EguiState::from_size(plugin303::gui::GUI_WIDTH, plugin303::gui::GUI_HEIGHT);
    let current_step = Arc::new(AtomicU8::new(255));

    let editor = plugin303::gui::create_editor(params, state, current_step);
    assert!(editor.is_some(), "GUI editor should construct successfully");
}

use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard<'a> {
    key: &'a str,
}

impl<'a> EnvGuard<'a> {
    fn set(key: &'a str, val: &str) -> Self {
        unsafe { std::env::set_var(key, val) };
        Self { key }
    }
}

impl<'a> Drop for EnvGuard<'a> {
    fn drop(&mut self) {
        unsafe { std::env::remove_var(self.key) };
    }
}

#[test]
fn test_qa_running_state_playhead_preserved() {
    use nih_plug::prelude::Plugin;
    let _lock = ENV_LOCK.lock().unwrap();
    let _guard = EnvGuard::set("VIBE303_QA_STATE", "running");

    let mut plugin = plugin303::Plugin303::default();
    assert_eq!(
        plugin
            .current_playing_step
            .load(std::sync::atomic::Ordering::Relaxed),
        3,
        "Running QA state must initialize playhead to step index 3"
    );
    plugin.reset();
    assert_eq!(
        plugin
            .current_playing_step
            .load(std::sync::atomic::Ordering::Relaxed),
        3,
        "Reset in running QA state must preserve active playhead index"
    );
}

#[test]
fn qa_playhead_pin_follows_sequencer_states_only() {
    for state in ["running", "running_note"] {
        assert_eq!(
            plugin303::qa_playhead_pin(Some(state)),
            Some(3),
            "{state} must pin the playhead to step 3"
        );
    }
    for state in [
        None,
        Some("default"),
        Some("accurate"),
        Some("knobs"),
        Some("runing"), // typo: an unknown state must not pin
    ] {
        assert_eq!(
            plugin303::qa_playhead_pin(state),
            None,
            "{state:?} must not pin the playhead"
        );
    }
}

#[test]
fn test_qa_running_note_state() {
    use nih_plug::prelude::Plugin;
    use plugin303::params::{Plugin303Params, StepGate};

    fn playhead(plugin: &plugin303::Plugin303) -> u8 {
        plugin
            .current_playing_step
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    let _lock = ENV_LOCK.lock().unwrap();
    let _guard = EnvGuard::set("VIBE303_QA_STATE", "running_note");

    let params = Plugin303Params::default();
    assert!(
        params.seq_run.value(),
        "running_note must start with the sequencer running"
    );
    assert!(
        params
            .pattern
            .read()
            .unwrap()
            .steps
            .iter()
            .all(|s| s.gate == StepGate::Note),
        "running_note must set every step to NOTE"
    );

    let mut plugin = plugin303::Plugin303::default();
    assert_eq!(
        playhead(&plugin),
        3,
        "running_note must initialise playhead to step 3"
    );
    plugin.reset();
    assert_eq!(
        playhead(&plugin),
        3,
        "reset must preserve the running_note playhead"
    );
}

#[test]
fn test_qa_running_state_keeps_default_pattern() {
    use plugin303::params::{Plugin303Params, StepGate};

    let _lock = ENV_LOCK.lock().unwrap();
    let _guard = EnvGuard::set("VIBE303_QA_STATE", "running");

    // The capture script documents step 3 as a REST in `running` and uses
    // `running_note` to get a NOTE under the playhead. `running` must not
    // pick up the all-NOTE pattern.
    let params = Plugin303Params::default();
    let pattern = params.pattern.read().unwrap();
    assert_eq!(pattern.steps[3].gate, StepGate::Rest);
    assert_eq!(pattern.steps[1].gate, StepGate::Tie);
}

#[test]
fn test_qa_quality_states_select_their_tier() {
    use plugin303::dsp::filter::Tier;
    use plugin303::params::Plugin303Params;

    let _lock = ENV_LOCK.lock().unwrap();
    for (state, tier) in [
        ("quality_low", Tier::Low),
        ("quality_high", Tier::High),
        ("quality_best", Tier::Best),
    ] {
        let _guard = EnvGuard::set("VIBE303_QA_STATE", state);
        assert_eq!(
            Plugin303Params::default().tier.value(),
            tier,
            "{state} must initialize the tier of the same name"
        );
    }
}

#[test]
fn test_qa_knobs_state() {
    use plugin303::params::Plugin303Params;

    let _lock = ENV_LOCK.lock().unwrap();
    let _guard = EnvGuard::set("VIBE303_QA_STATE", "knobs");

    let params = Plugin303Params::default();
    assert_eq!(
        params.tuning.value(),
        25.0,
        "knobs QA state must initialize Tuning to 25.0 cents"
    );
    assert_eq!(
        params.cutoff.value(),
        0.75,
        "knobs QA state must initialize Cutoff to 0.75"
    );
}

#[cfg(feature = "gui")]
#[test]
fn test_qa_generator_state_editor_build() {
    use plugin303::params::Plugin303Params;
    use std::sync::atomic::AtomicU8;
    use std::sync::Arc;

    let _lock = ENV_LOCK.lock().unwrap();
    let _guard_gen = EnvGuard::set("VIBE303_QA_STATE", "generator");

    let params = Arc::new(Plugin303Params::default());
    let state =
        nih_plug_egui::EguiState::from_size(plugin303::gui::GUI_WIDTH, plugin303::gui::GUI_HEIGHT);
    let current_step = Arc::new(AtomicU8::new(255));

    let editor = plugin303::gui::create_editor(params, state, current_step);
    assert!(
        editor.is_some(),
        "Editor must build cleanly under generator QA state"
    );
}
