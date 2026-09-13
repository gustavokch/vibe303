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

#[cfg(feature = "gui")]
#[test]
fn test_gui_panel_width_budget() {
    let window_width = plugin303::gui::GUI_WIDTH;
    let window_height = plugin303::gui::GUI_HEIGHT;
    assert_eq!(window_width, 1366);
    assert_eq!(window_height, 960);
    let inner_content_width = window_width - 20; // 10px left/right margin
    assert_eq!(inner_content_width, 1346);
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
fn test_qa_quality_high_state() {
    use plugin303::dsp::filter::Tier;
    use plugin303::params::Plugin303Params;

    let _lock = ENV_LOCK.lock().unwrap();
    let _guard = EnvGuard::set("VIBE303_QA_STATE", "quality_high");

    let params = Plugin303Params::default();
    assert_eq!(
        params.tier.value(),
        Tier::High,
        "quality_high QA state must initialize Tier to High"
    );
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
fn test_qa_generator_and_zoom_state_editor_build() {
    use plugin303::params::Plugin303Params;
    use std::sync::atomic::AtomicU8;
    use std::sync::Arc;

    let _lock = ENV_LOCK.lock().unwrap();
    let _guard_gen = EnvGuard::set("VIBE303_QA_STATE", "generator");

    let params = Arc::new(Plugin303Params::default());
    let state =
        nih_plug_egui::EguiState::from_size(plugin303::gui::GUI_WIDTH, plugin303::gui::GUI_HEIGHT);
    let current_step = Arc::new(AtomicU8::new(255));

    let editor = plugin303::gui::create_editor(params.clone(), state.clone(), current_step.clone());
    assert!(
        editor.is_some(),
        "Editor must build cleanly under generator QA state"
    );
    drop(_guard_gen);

    let _guard_zoom = EnvGuard::set("VIBE303_QA_STATE", "zoom75");
    let editor_zoom = plugin303::gui::create_editor(params, state, current_step);
    assert!(
        editor_zoom.is_some(),
        "Editor must build cleanly under zoom75 QA state"
    );
}
