use nih_plug_egui::egui::{Color32, Stroke};

pub const CHASSIS_BG: Color32 = Color32::from_rgb(0x13, 0x13, 0x15);
pub const PANEL_BG: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1e);
pub const PANEL_BG_SOUPED: Color32 = Color32::from_rgb(0x18, 0x18, 0x1c);
pub const WELL_BG: Color32 = Color32::from_rgb(0x16, 0x16, 0x19);
pub const SURFACE_ELEVATED: Color32 = Color32::from_rgb(0x22, 0x22, 0x28);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x33);

pub const BORDER_DEFAULT: Color32 = Color32::from_rgb(0x2b, 0x2b, 0x34);
pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x24);
pub const BORDER_FOCUS: Color32 = Color32::from_rgb(0x40, 0x40, 0x50);
pub const BORDER_ACCENT: Color32 = Color32::from_rgb(0xff, 0x7a, 0x00);

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xff, 0xff, 0xff);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0xc4, 0xc4, 0xd0);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8f, 0x8f, 0xa4);
pub const TEXT_ON_ACCENT: Color32 = Color32::from_rgb(0x0b, 0x0b, 0x0d);

pub const ACCENT_ORANGE: Color32 = Color32::from_rgb(0xff, 0x7a, 0x00);
pub const ACCENT_CYAN: Color32 = Color32::from_rgb(0x00, 0xf0, 0xff);
pub const ACCENT_GOLD: Color32 = Color32::from_rgb(0xff, 0xc8, 0x3b);
pub const ACCENT_SLATE: Color32 = Color32::from_rgb(0x2c, 0x2d, 0x36);
pub const ACCENT_RUBY: Color32 = Color32::from_rgb(0xff, 0x3b, 0x56);

// Backward-compatibility aliases during transition
pub const PANEL_BORDER: Color32 = BORDER_DEFAULT;
pub const PANEL_BORDER_HIGHLIGHT: Color32 = BORDER_FOCUS;
pub const BANK_FRAME_BG: Color32 = WELL_BG;
pub const BANK_FRAME_BORDER: Color32 = BORDER_DEFAULT;
pub const BTN_ACTIVE_BG: Color32 = SURFACE_ELEVATED;
pub const BTN_INACTIVE_BG: Color32 = WELL_BG;
pub const ACCENT_AMBER: Color32 = ACCENT_ORANGE;
pub const ACCENT_AMBER_GLOW: Color32 = Color32::from_rgba_premultiplied(0xff, 0x7a, 0x00, 0x40);
pub const SLIDE_CYAN: Color32 = ACCENT_CYAN;
pub const SLIDE_CYAN_GLOW: Color32 = Color32::from_rgba_premultiplied(0x00, 0xf0, 0xff, 0x40);
pub const PLAYHEAD_RED: Color32 = ACCENT_ORANGE;
pub const PLAYHEAD_RED_GLOW: Color32 = Color32::from_rgba_premultiplied(0xff, 0x7a, 0x00, 0x40);
pub const SOUPED_ORANGE: Color32 = ACCENT_ORANGE;
pub const GATE_NOTE_COLOR: Color32 = ACCENT_CYAN;
pub const GATE_TIE_COLOR: Color32 = ACCENT_GOLD;
pub const GATE_REST_COLOR: Color32 = ACCENT_SLATE;

pub const KNOB_BODY_LIGHT: Color32 = SURFACE_ELEVATED;
pub const KNOB_BODY_DARK: Color32 = WELL_BG;
pub const KNOB_BEZEL: Color32 = BORDER_DEFAULT;
pub const KNOB_POINTER: Color32 = ACCENT_ORANGE;

pub fn border_stroke() -> Stroke {
    Stroke::new(1.0_f32, BORDER_DEFAULT)
}

pub fn highlight_stroke() -> Stroke {
    Stroke::new(1.0_f32, BORDER_FOCUS)
}
