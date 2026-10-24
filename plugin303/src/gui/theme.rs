//! Flat device-chain theme: Bitwig Studio's charcoal planes and orange accent,
//! drawn with Ableton Live's flat vocabulary (tone-on-tone planes, no outlines,
//! no gradients, no shadows).
//!
//! Depth comes from five neutral planes only. Orange marks live state: value
//! arcs, lit toggles, the selected segment, the playhead. A second hue is
//! allowed only where a fixed role needs it (`ACCENT_BLUE` = slide).

use nih_plug_egui::egui::{self, Color32, CornerRadius, FontId, Shadow, Stroke, TextStyle, Ui};

// ── Planes (darkest to lightest) ────────────────────────────────────────────
/// Window plane. Shows through as the gutter between devices.
pub const CHASSIS_BG: Color32 = Color32::from_rgb(0x15, 0x15, 0x15);
/// Inset plane: value fields, fader tracks, empty step cells, dividers.
pub const WELL_BG: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1a);
/// Device body.
pub const PANEL_BG: Color32 = Color32::from_rgb(0x26, 0x26, 0x26);
/// Device header strip.
pub const PANEL_HEADER_BG: Color32 = Color32::from_rgb(0x2f, 0x2f, 0x2f);
/// Control at rest: buttons, unselected segments, knob body.
pub const SURFACE_ELEVATED: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x3a);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0x49, 0x49, 0x49);
pub const SURFACE_PRESSED: Color32 = Color32::from_rgb(0x30, 0x30, 0x30);

// ── Text ────────────────────────────────────────────────────────────────────
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xec, 0xec, 0xec);
/// Safe on every plane including `SURFACE_ELEVATED` and `PANEL_HEADER_BG`.
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0xb8, 0xb8, 0xb8);
/// Safe on `CHASSIS_BG`, `WELL_BG` and `PANEL_BG` only. Below 4.5:1 on
/// `PANEL_HEADER_BG` and `SURFACE_ELEVATED`.
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8f, 0x8f, 0x8f);
/// Text on `ACCENT_ORANGE`, `ACCENT_BLUE` and the light step-gate fills.
pub const TEXT_ON_ACCENT: Color32 = Color32::from_rgb(0x14, 0x14, 0x14);

// ── Accent ──────────────────────────────────────────────────────────────────
/// Bitwig CI orange.
pub const ACCENT_ORANGE: Color32 = Color32::from_rgb(0xf6, 0x66, 0x1e);
pub const ACCENT_ORANGE_HOVER: Color32 = Color32::from_rgb(0xff, 0x7f, 0x3a);
/// Bitwig blue. Fixed role: a lit SLIDE flag.
pub const ACCENT_BLUE: Color32 = Color32::from_rgb(0x5c, 0xa8, 0xee);

// ── Step gate fills ─────────────────────────────────────────────────────────
// Neutral on purpose: orange is reserved for ACCENT, blue for SLIDE, so the
// rhythm skeleton reads from the gate row alone. NOTE > TIE > REST in
// brightness. NOTE takes dark text, TIE takes light text.
pub const GATE_NOTE_FILL: Color32 = Color32::from_rgb(0xa8, 0xa8, 0xa8);
pub const GATE_TIE_FILL: Color32 = Color32::from_rgb(0x5f, 0x5f, 0x5f);

// ── Knob ────────────────────────────────────────────────────────────────────
pub const KNOB_BODY: Color32 = SURFACE_ELEVATED;
pub const KNOB_BODY_HOVER: Color32 = SURFACE_HOVER;
/// Unfilled part of the value ring.
pub const KNOB_TRACK: Color32 = Color32::from_rgb(0x40, 0x40, 0x40);

// Knob size tiers: outer diameter LARGE 42 pt, MEDIUM 24 pt.
pub const KNOB_RADIUS_LARGE: f32 = 21.0;
pub const KNOB_RADIUS_MEDIUM: f32 = 12.0;

// Value-field widths matching the knob tiers.
pub const KNOB_PILL_WIDTH_LARGE: f32 = 52.0;
pub const KNOB_PILL_WIDTH_MEDIUM: f32 = 38.0;
pub const KNOB_PILL_WIDTH_SMALL: f32 = 34.0;

// ── Shape ───────────────────────────────────────────────────────────────────
/// Every panel, button and field uses this one radius.
pub const RADIUS: u8 = 2;

pub fn radius() -> CornerRadius {
    CornerRadius::same(RADIUS)
}

/// Scope style for numeric fields and dropdowns: an inset well instead of a
/// raised button, so a value you type reads differently from an action you click.
pub fn field_style(ui: &mut Ui) {
    let w = &mut ui.style_mut().visuals.widgets;
    w.inactive.weak_bg_fill = WELL_BG;
    w.inactive.bg_fill = WELL_BG;
    w.inactive.fg_stroke.color = TEXT_PRIMARY;
    w.hovered.weak_bg_fill = SURFACE_PRESSED;
    w.hovered.bg_fill = SURFACE_PRESSED;
    w.active.weak_bg_fill = SURFACE_PRESSED;
    w.active.bg_fill = SURFACE_PRESSED;
    w.open.weak_bg_fill = SURFACE_PRESSED;
    w.open.bg_fill = SURFACE_PRESSED;
}

/// Install the flat style on the egui context. Built-in widgets (DragValue,
/// ComboBox, Button) inherit it, so panels only draw what is specific to them.
pub fn apply(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
    ctx.style_mut(|style| {
        // One font rule: every numeric readout (every DragValue) renders in the
        // monospace face at the compact size; word-valued controls keep the
        // proportional face.
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(9.0));
        style.drag_value_text_style = TextStyle::Monospace;
        // Unstyled text (combo values, plain buttons, labels) follows the same
        // compact scale instead of egui's 14 pt default.
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(9.5));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(9.5));

        style.spacing.item_spacing = egui::vec2(6.0, 3.0);
        style.spacing.button_padding = egui::vec2(8.0, 1.0);
        style.spacing.interact_size = egui::vec2(24.0, 18.0);

        let v = &mut style.visuals;
        v.panel_fill = CHASSIS_BG;
        v.window_fill = SURFACE_ELEVATED;
        v.window_stroke = Stroke::NONE;
        v.window_corner_radius = radius();
        v.menu_corner_radius = radius();
        v.window_shadow = Shadow::NONE;
        v.popup_shadow = Shadow::NONE;
        v.extreme_bg_color = WELL_BG;
        v.faint_bg_color = PANEL_BG;
        v.selection.bg_fill = ACCENT_ORANGE;
        v.selection.stroke = Stroke::new(1.0_f32, TEXT_ON_ACCENT);
        v.text_cursor.stroke = Stroke::new(1.5_f32, ACCENT_ORANGE);

        let w = &mut v.widgets;
        w.noninteractive.bg_fill = PANEL_BG;
        w.noninteractive.weak_bg_fill = PANEL_BG;
        // Separators use this stroke: a 1 px dark line, the only line in the UI.
        w.noninteractive.bg_stroke = Stroke::new(1.0_f32, WELL_BG);
        w.noninteractive.fg_stroke.color = TEXT_SECONDARY;

        for (state, fill, text) in [
            (&mut w.inactive, SURFACE_ELEVATED, TEXT_SECONDARY),
            (&mut w.hovered, SURFACE_HOVER, TEXT_PRIMARY),
            (&mut w.active, SURFACE_PRESSED, TEXT_PRIMARY),
            (&mut w.open, SURFACE_HOVER, TEXT_PRIMARY),
        ] {
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.bg_stroke = Stroke::NONE;
            state.fg_stroke = Stroke::new(1.0_f32, text);
            state.corner_radius = radius();
            state.expansion = 0.0;
        }
        w.noninteractive.corner_radius = radius();
    });
}

/// WCAG relative luminance of an sRGB colour.
pub fn rel_luminance(c: Color32) -> f64 {
    let to_linear = |v: u8| {
        let s = v as f64 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * to_linear(c.r()) + 0.7152 * to_linear(c.g()) + 0.0722 * to_linear(c.b())
}

/// WCAG contrast ratio of two colours, in either order.
pub fn contrast_ratio(c1: Color32, c2: Color32) -> f64 {
    let l1 = rel_luminance(c1);
    let l2 = rel_luminance(c2);
    let (hi, lo) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every text token must clear WCAG AA (4.5:1) on every plane it is
    /// documented as safe for. Small caps labels at 8.5 pt need the full ratio.
    #[test]
    fn text_tokens_clear_aa_on_their_planes() {
        let on_all = [
            CHASSIS_BG,
            WELL_BG,
            PANEL_BG,
            PANEL_HEADER_BG,
            SURFACE_ELEVATED,
            SURFACE_HOVER,
        ];
        for plane in on_all {
            for text in [TEXT_PRIMARY, TEXT_SECONDARY] {
                assert!(
                    contrast_ratio(text, plane) >= 4.5,
                    "{text:?} on {plane:?} = {:.2}",
                    contrast_ratio(text, plane)
                );
            }
        }
        for plane in [CHASSIS_BG, WELL_BG, PANEL_BG] {
            assert!(
                contrast_ratio(TEXT_MUTED, plane) >= 4.5,
                "TEXT_MUTED on {plane:?} = {:.2}",
                contrast_ratio(TEXT_MUTED, plane)
            );
        }
    }

    /// Dark text sits on every lit fill (accent, slide, light gate cells).
    #[test]
    fn text_on_accent_clears_aa_on_every_lit_fill() {
        for fill in [
            ACCENT_ORANGE,
            ACCENT_ORANGE_HOVER,
            ACCENT_BLUE,
            GATE_NOTE_FILL,
        ] {
            assert!(
                contrast_ratio(TEXT_ON_ACCENT, fill) >= 4.5,
                "TEXT_ON_ACCENT on {fill:?} = {:.2}",
                contrast_ratio(TEXT_ON_ACCENT, fill)
            );
        }
        assert!(
            contrast_ratio(TEXT_PRIMARY, GATE_TIE_FILL) >= 4.5,
            "TEXT_PRIMARY on the TIE fill = {:.2}",
            contrast_ratio(TEXT_PRIMARY, GATE_TIE_FILL)
        );
    }

    /// Orange as a foreground (value arc, selected text) must stay visible on
    /// the device body: 3:1 is the non-text floor.
    #[test]
    fn accent_is_visible_on_device_body() {
        assert!(contrast_ratio(ACCENT_ORANGE, PANEL_BG) >= 3.0);
        assert!(contrast_ratio(ACCENT_ORANGE, PANEL_HEADER_BG) >= 3.0);
    }

    /// Interaction states must be ordered: hover lighter than rest, pressed
    /// darker than rest.
    #[test]
    fn control_states_are_ordered() {
        assert!(rel_luminance(SURFACE_PRESSED) < rel_luminance(SURFACE_ELEVATED));
        assert!(rel_luminance(SURFACE_ELEVATED) < rel_luminance(SURFACE_HOVER));
    }

    /// Planes must step upward so each nesting level is distinguishable
    /// without an outline.
    #[test]
    fn planes_step_upward() {
        let planes = [
            CHASSIS_BG,
            WELL_BG,
            PANEL_BG,
            PANEL_HEADER_BG,
            SURFACE_ELEVATED,
        ];
        for pair in planes.windows(2) {
            assert!(
                rel_luminance(pair[0]) < rel_luminance(pair[1]),
                "{:?} must be darker than {:?}",
                pair[0],
                pair[1]
            );
        }
    }
}
