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
    assert_eq!(window_width, 1080);
    assert_eq!(window_height, 720);
    let inner_content_width = window_width - 16; // 8px left/right margin
    assert_eq!(inner_content_width, 1064);
}
