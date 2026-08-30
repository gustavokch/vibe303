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
    assert_eq!(TEXT_MUTED, Color32::from_rgb(0x8f, 0x8f, 0xa4));
    assert_eq!(TEXT_ON_ACCENT, Color32::from_rgb(0x0b, 0x0b, 0x0d));
}

#[cfg(feature = "gui")]
#[test]
fn test_text_muted_contrast_wcag_aa() {
    use nih_plug_egui::egui::Color32;
    use plugin303::gui::theme::*;

    fn relative_luminance(c: Color32) -> f32 {
        fn channel_lum(val: u8) -> f32 {
            let s = val as f32 / 255.0;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel_lum(c.r()) + 0.7152 * channel_lum(c.g()) + 0.0722 * channel_lum(c.b())
    }

    fn contrast_ratio(c1: Color32, c2: Color32) -> f32 {
        let l1 = relative_luminance(c1);
        let l2 = relative_luminance(c2);
        let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
        (lighter + 0.05) / (darker + 0.05)
    }

    let ratio_panel = contrast_ratio(TEXT_MUTED, PANEL_BG);
    assert!(
        ratio_panel >= 4.5,
        "TEXT_MUTED on PANEL_BG contrast ratio {} < 4.5",
        ratio_panel
    );

    let ratio_chassis = contrast_ratio(TEXT_MUTED, CHASSIS_BG);
    assert!(
        ratio_chassis >= 4.5,
        "TEXT_MUTED on CHASSIS_BG contrast ratio {} < 4.5",
        ratio_chassis
    );
}
