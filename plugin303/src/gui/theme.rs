use nih_plug_egui::egui::{Color32, Stroke};

pub const CHASSIS_BG: Color32 = Color32::from_rgb(0x16, 0x18, 0x1b);
pub const PANEL_BG: Color32 = Color32::from_rgb(0x1e, 0x22, 0x27);
pub const PANEL_BG_SOUPED: Color32 = Color32::from_rgb(0x12, 0x15, 0x1a);
pub const PANEL_BORDER: Color32 = Color32::from_rgb(0x32, 0x38, 0x42);
pub const PANEL_BORDER_HIGHLIGHT: Color32 = Color32::from_rgb(0x48, 0x52, 0x60);

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xee, 0xf0, 0xf2);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0x9a, 0xa2, 0xad);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x5a, 0x62, 0x6e);

pub const KNOB_BODY_LIGHT: Color32 = Color32::from_rgb(0xd2, 0xd6, 0xdc);
pub const KNOB_BODY_DARK: Color32 = Color32::from_rgb(0x84, 0x8a, 0x94);
pub const KNOB_BEZEL: Color32 = Color32::from_rgb(0x28, 0x2c, 0x34);
pub const KNOB_POINTER: Color32 = Color32::from_rgb(0xff, 0xff, 0xff);

pub const ACCENT_AMBER: Color32 = Color32::from_rgb(0xff, 0xaa, 0x00);
pub const ACCENT_AMBER_GLOW: Color32 = Color32::from_rgba_premultiplied(0xff, 0xaa, 0x00, 0x40);

pub const SLIDE_CYAN: Color32 = Color32::from_rgb(0x00, 0xe5, 0xff);
pub const SLIDE_CYAN_GLOW: Color32 = Color32::from_rgba_premultiplied(0x00, 0xe5, 0xff, 0x40);

pub const PLAYHEAD_RED: Color32 = Color32::from_rgb(0xff, 0x33, 0x55);
pub const PLAYHEAD_RED_GLOW: Color32 = Color32::from_rgba_premultiplied(0xff, 0x33, 0x55, 0x60);

pub const SOUPED_ORANGE: Color32 = Color32::from_rgb(0xff, 0x77, 0x00);
pub const GATE_NOTE_COLOR: Color32 = Color32::from_rgb(0x00, 0xe5, 0xff);
pub const GATE_TIE_COLOR: Color32 = Color32::from_rgb(0xff, 0xd7, 0x00);
pub const GATE_REST_COLOR: Color32 = Color32::from_rgb(0x38, 0x40, 0x4c);

pub fn border_stroke() -> Stroke {
    Stroke::new(1.0_f32, PANEL_BORDER)
}

pub fn highlight_stroke() -> Stroke {
    Stroke::new(1.0_f32, PANEL_BORDER_HIGHLIGHT)
}
