use nih_plug_egui::egui::{Color32, Stroke};

pub const CHASSIS_BG: Color32 = Color32::from_rgb(0x18, 0x18, 0x18);
pub const PANEL_BG: Color32 = Color32::from_rgb(0x24, 0x24, 0x24);
pub const PANEL_BG_SOUPED: Color32 = Color32::from_rgb(0x20, 0x20, 0x20);
pub const PANEL_BORDER: Color32 = Color32::from_rgb(0x38, 0x38, 0x38);
pub const PANEL_BORDER_HIGHLIGHT: Color32 = Color32::from_rgb(0x4a, 0x4a, 0x4a);

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xf8, 0xfa, 0xfc);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0x94, 0xa3, 0xb8);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x64, 0x74, 0x8b);

pub const KNOB_BODY_LIGHT: Color32 = Color32::from_rgb(0xd0, 0xd4, 0xd8);
pub const KNOB_BODY_DARK: Color32 = Color32::from_rgb(0x88, 0x8e, 0x96);
pub const KNOB_BEZEL: Color32 = Color32::from_rgb(0x28, 0x28, 0x28);
pub const KNOB_POINTER: Color32 = Color32::from_rgb(0xff, 0x77, 0x00);

pub const ACCENT_AMBER: Color32 = Color32::from_rgb(0xff, 0x77, 0x00);
pub const ACCENT_AMBER_GLOW: Color32 = Color32::from_rgba_premultiplied(0xff, 0x77, 0x00, 0x40);

pub const SLIDE_CYAN: Color32 = Color32::from_rgb(0x38, 0xbd, 0xf8);
pub const SLIDE_CYAN_GLOW: Color32 = Color32::from_rgba_premultiplied(0x38, 0xbd, 0xf8, 0x40);

pub const PLAYHEAD_RED: Color32 = Color32::from_rgb(0xff, 0x77, 0x00);
pub const PLAYHEAD_RED_GLOW: Color32 = Color32::from_rgba_premultiplied(0xff, 0x77, 0x00, 0x60);

pub const SOUPED_ORANGE: Color32 = Color32::from_rgb(0xff, 0x77, 0x00);
pub const GATE_NOTE_COLOR: Color32 = Color32::from_rgb(0x00, 0xe5, 0xff);
pub const GATE_TIE_COLOR: Color32 = Color32::from_rgb(0xf5, 0x9e, 0x0b);
pub const GATE_REST_COLOR: Color32 = Color32::from_rgb(0x33, 0x3a, 0x42);

pub fn border_stroke() -> Stroke {
    Stroke::new(1.0_f32, PANEL_BORDER)
}

pub fn highlight_stroke() -> Stroke {
    Stroke::new(1.0_f32, PANEL_BORDER_HIGHLIGHT)
}
