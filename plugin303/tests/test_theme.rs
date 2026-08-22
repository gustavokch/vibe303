#[cfg(feature = "gui")]
#[test]
fn test_theme_color_tokens() {
    use nih_plug_egui::egui::Color32;
    use plugin303::gui::theme::*;

    assert_eq!(CHASSIS_BG, Color32::from_rgb(0x13, 0x13, 0x15));
    assert_eq!(PANEL_BG, Color32::from_rgb(0x1a, 0x1a, 0x1e));
    assert_eq!(WELL_BG, Color32::from_rgb(0x16, 0x16, 0x19));
    assert_eq!(ACCENT_ORANGE, Color32::from_rgb(0xff, 0x7a, 0x00));
    assert_eq!(ACCENT_CYAN, Color32::from_rgb(0x00, 0xf0, 0xff));
    assert_eq!(ACCENT_GOLD, Color32::from_rgb(0xff, 0xc8, 0x3b));
    assert_eq!(ACCENT_SLATE, Color32::from_rgb(0x2c, 0x2d, 0x36));
    assert_eq!(ACCENT_RUBY, Color32::from_rgb(0xff, 0x3b, 0x56));
    assert_eq!(TEXT_PRIMARY, Color32::from_rgb(0xff, 0xff, 0xff));
    assert_eq!(TEXT_SECONDARY, Color32::from_rgb(0xc4, 0xc4, 0xd0));
    assert_eq!(TEXT_MUTED, Color32::from_rgb(0x6e, 0x6e, 0x80));
    assert_eq!(TEXT_ON_ACCENT, Color32::from_rgb(0x0b, 0x0b, 0x0d));
}
