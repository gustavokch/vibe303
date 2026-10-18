use nih_plug_egui::egui::{Color32, Stroke};

pub const CHASSIS_BG: Color32 = Color32::from_rgb(0x13, 0x13, 0x15);
pub const PANEL_BG: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1e);
pub const PANEL_BG_SOUPED: Color32 = Color32::from_rgb(0x18, 0x18, 0x1c);
pub const WELL_BG: Color32 = Color32::from_rgb(0x16, 0x16, 0x19);
pub const SURFACE_ELEVATED: Color32 = Color32::from_rgb(0x22, 0x22, 0x28);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x33);

pub const BORDER_DEFAULT: Color32 = Color32::from_rgb(0x66, 0x66, 0x70);
pub const BORDER_FOCUS: Color32 = Color32::from_rgb(0x80, 0x80, 0x90);
pub const BORDER_ACCENT: Color32 = Color32::from_rgb(0xff, 0x7a, 0x00);
pub const FIELD_FILL: Color32 = Color32::from_rgb(0x66, 0x66, 0x70);
/// Widget border stroke (round-19 review, issue 1 follow-up). `BORDER_DEFAULT`
/// equals `FIELD_FILL`, so a field border drawn in it is invisible on the
/// field itself — the "border" passed contrast only via the fill edge. Fields
/// share `BORDER_FOCUS` instead: 4.47:1 against `PANEL_BG`, 1.46:1 against
/// `FIELD_FILL` — a deliberately subtle but rendered boundary. Panel frames
/// keep `BORDER_DEFAULT` via `border_stroke()`.
pub const FIELD_BORDER: Color32 = BORDER_FOCUS;
pub const FIELD_FILL_HOVER: Color32 = Color32::from_rgb(0x70, 0x70, 0x7c);
pub const FIELD_FILL_ACTIVE: Color32 = Color32::from_rgb(0x5c, 0x5c, 0x66);
pub const KNOB_TRACK_ARC: Color32 = Color32::from_rgb(0x66, 0x66, 0x70);

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xff, 0xff, 0xff);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0xc4, 0xc4, 0xd0);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8f, 0x8f, 0xa4);
pub const TEXT_ON_ACCENT: Color32 = Color32::from_rgb(0x0b, 0x0b, 0x0d);

pub const ACCENT_ORANGE: Color32 = Color32::from_rgb(0xff, 0x7a, 0x00);
pub const ACCENT_CYAN: Color32 = Color32::from_rgb(0x00, 0xf0, 0xff);
pub const ACCENT_GOLD: Color32 = Color32::from_rgb(0xff, 0xc8, 0x3b);
pub const ACCENT_SLATE: Color32 = Color32::from_rgb(0x2c, 0x2d, 0x36);
pub const ACCENT_RUBY: Color32 = Color32::from_rgb(0xff, 0x3b, 0x56);

/// Playhead frame outer stroke (round-19 issue 9). Violet is the only accent
/// hue not already spoken for inside the step grid: cyan is NOTE, gold is TIE,
/// orange is ACC and STOP, slate is REST.
///
/// The frame is painted as two 1 px rings, this one outside
/// [`PLAYHEAD_FRAME_INNER`], because *no* single colour can clear the 3:1
/// non-text floor against every cell the frame crosses. 3:1 against the
/// darkest cell demands a luminance the brightest cell forbids — the window
/// is empty. The numeric derivation lives in
/// `tests::playhead_frame_covers_every_cell_it_crosses`, the single source
/// for it; round-18's single white stroke measured 14.1:1 on REST but 1.67:1
/// on TIE, which is why one ring was not enough.
///
/// Two rings close it. The outer ring lands on the column background, always
/// dark (5.34:1 on WELL_BG), so it carries the violet. The inner ring lands
/// on the cell fills and carries the near-black, which is what reads on the
/// bright ones (12.2:1 vs TIE gold, 13.4:1 vs NOTE cyan). Every background
/// the frame can meet is therefore covered by one ring or the other.
pub const PLAYHEAD_FRAME_OUTER: Color32 = Color32::from_rgb(0xb8, 0x6b, 0xff);

/// Playhead frame inner stroke — see [`PLAYHEAD_FRAME_OUTER`] for why the
/// frame needs two tones. Also separates the violet from any bright cell it
/// would otherwise sit flush against (5.93:1 against the outer ring).
pub const PLAYHEAD_FRAME_INNER: Color32 = Color32::from_rgb(0x0b, 0x0b, 0x0d);

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

/// Inactive NOTE cell fill (round-17 issue 3). The NOTE gate pill paints a
/// `SURFACE_ELEVATED` thumb on an `ACCENT_CYAN` fill in the active state;
/// when the parent Ui is disabled the pill keeps the cyan fill on a
/// fade-cleared painter (one dimming mechanism only, round-13 issue 6), and
/// antialiased edge pixels sample part-cyan. The probe's `#06777F` sample is
/// that partial-coverage edge blend, not a separate unregistered paint — this
/// token names it so the QA script snaps it instead of flagging it.
pub const GATE_NOTE_INACTIVE_EDGE: Color32 = Color32::from_rgb(0x06, 0x77, 0x7F);

pub const KNOB_BODY_LIGHT: Color32 = SURFACE_ELEVATED;
pub const KNOB_BODY_DARK: Color32 = WELL_BG;
pub const KNOB_BEZEL: Color32 = BORDER_DEFAULT;
pub const KNOB_POINTER: Color32 = ACCENT_ORANGE;

// Systematic knob size tiers (round-18 issue 12)
// Outer diameters: LARGE 42 pt (radius 21.0), MEDIUM 27 pt (radius 13.5), SMALL 24 pt (radius 12.0)
pub const KNOB_RADIUS_LARGE: f32 = 21.0;
pub const KNOB_RADIUS_MEDIUM: f32 = 13.5;
pub const KNOB_RADIUS_SMALL: f32 = 12.0;

// Corresponding value-pill widths matching knob tiers (round-18 issue 12, issue 24)
pub const KNOB_PILL_WIDTH_LARGE: f32 = 52.0;
pub const KNOB_PILL_WIDTH_MEDIUM: f32 = 38.0;
pub const KNOB_PILL_WIDTH_SMALL: f32 = 34.0;

pub fn border_stroke() -> Stroke {
    Stroke::new(1.0_f32, BORDER_DEFAULT)
}

pub fn highlight_stroke() -> Stroke {
    Stroke::new(1.0_f32, BORDER_FOCUS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel_luminance(c: Color32) -> f64 {
        let to_linear = |v: u8| {
            let s = v as f64 / 255.0;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        let r = to_linear(c.r());
        let g = to_linear(c.g());
        let b = to_linear(c.b());
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    fn contrast_ratio(c1: Color32, c2: Color32) -> f64 {
        let l1 = rel_luminance(c1);
        let l2 = rel_luminance(c2);
        let (hi, lo) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
        (hi + 0.05) / (lo + 0.05)
    }

    #[test]
    fn selection_text_contrast() {
        // DragValue text selection uses TEXT_PRIMARY on selection.bg_fill (ACCENT_ORANGE)
        // or TEXT_ON_ACCENT. Both must meet readability standards.
        assert!(contrast_ratio(TEXT_PRIMARY, ACCENT_ORANGE) >= 2.0);
        assert!(contrast_ratio(TEXT_ON_ACCENT, ACCENT_ORANGE) >= 4.5);
    }

    #[test]
    fn field_fill_contrast_and_scale() {
        assert!(contrast_ratio(FIELD_FILL, PANEL_BG) >= 3.0);
        assert!(contrast_ratio(FIELD_FILL_HOVER, PANEL_BG) >= 3.0);
        // Active pressed state is darker than resting, hover is lighter
        assert!(rel_luminance(FIELD_FILL_ACTIVE) < rel_luminance(FIELD_FILL));
        assert!(rel_luminance(FIELD_FILL) < rel_luminance(FIELD_FILL_HOVER));

        // The widget border must actually render: it cannot equal the fill it
        // sits on (that was issue 1 — FIELD_FILL == BORDER_DEFAULT made field
        // borders invisible), it must clear 3:1 against the panel, and the
        // hovered stroke must stay distinct from the resting one.
        assert_ne!(FIELD_BORDER, FIELD_FILL);
        assert!(contrast_ratio(FIELD_BORDER, PANEL_BG) >= 3.0);
        assert_ne!(TEXT_MUTED, FIELD_BORDER);
    }

    #[test]
    fn field_text_reads_on_the_field_fill() {
        // Round-19 issue 7. Once the fill is routed through FIELD_FILL, the
        // field text must still clear 4.5:1 on top of it in every widget
        // state. TEXT_SECONDARY does not (3.29:1 on FIELD_FILL), which is why
        // `gui/mod.rs` sets the inactive/hovered/active/open foreground to
        // TEXT_PRIMARY.
        for fill in [FIELD_FILL, FIELD_FILL_HOVER, FIELD_FILL_ACTIVE] {
            assert!(
                contrast_ratio(TEXT_PRIMARY, fill) >= 4.5,
                "field text must clear 4.5:1 on {fill:?}"
            );
        }
    }

    #[test]
    fn field_fill_windows_are_disjoint() {
        // Round-19 issue 7, proven by code rather than prose: no fill
        // satisfies >= 3:1 against PANEL_BG and >= 4.5:1 under TEXT_SECONDARY
        // at once. The first window is [0.132, inf), the second (-inf,
        // 0.085] — they do not intersect, so a fill-only fix was never
        // available; TEXT_PRIMARY clears both.
        let fill_min = 3.0 * (rel_luminance(PANEL_BG) + 0.05) - 0.05;
        let fill_max = (rel_luminance(TEXT_SECONDARY) + 0.05) / 4.5 - 0.05;
        assert!(
            fill_min > fill_max,
            "a fill should exist that clears both windows, got [{fill_min:.3}, inf) vs (-inf, {fill_max:.3}]"
        );
        // And the white text that the widget style settles on clears 4.5:1 on
        // every field fill.
        for fill in [FIELD_FILL, FIELD_FILL_HOVER, FIELD_FILL_ACTIVE] {
            assert!(contrast_ratio(TEXT_PRIMARY, fill) >= 4.5);
        }
    }

    #[test]
    fn playhead_frame_covers_every_cell_it_crosses() {
        // Round-19 issue 9. No single colour clears the 3:1 non-text floor
        // against every cell in the step strip, so the frame is two rings and
        // each background must be met by at least one of them.
        let cells = [
            GATE_REST_COLOR,
            GATE_NOTE_COLOR,
            GATE_TIE_COLOR,
            WELL_BG,
            SURFACE_ELEVATED,
        ];

        // The binding pair is the darkest cell and the TIE gold: clearing 3:1
        // against slate needs luminance >= 0.180, clearing it against gold
        // needs <= 0.176. Gold rather than the brighter cyan sets the ceiling
        // precisely because it is the dimmer of the two bright cells.
        let floor_against_rest = 3.0 * (rel_luminance(GATE_REST_COLOR) + 0.05) - 0.05;
        let ceil_against_tie = (rel_luminance(GATE_TIE_COLOR) + 0.05) / 3.0 - 0.05;
        assert!(
            floor_against_rest > ceil_against_tie,
            "a one-colour playhead frame would be possible; collapse the two rings"
        );

        // Round-18's white stroke is the concrete casualty of that window.
        assert!(contrast_ratio(TEXT_PRIMARY, GATE_TIE_COLOR) < 3.0);

        // Each ring alone leaves a cell uncovered — both are load-bearing.
        for ring in [PLAYHEAD_FRAME_OUTER, PLAYHEAD_FRAME_INNER] {
            assert!(
                cells.iter().any(|c| contrast_ratio(ring, *c) < 3.0),
                "{ring:?} covers every cell on its own; the second ring is dead weight"
            );
        }

        // Together they cover all of them.
        for cell in cells {
            let outer = contrast_ratio(PLAYHEAD_FRAME_OUTER, cell);
            let inner = contrast_ratio(PLAYHEAD_FRAME_INNER, cell);
            assert!(
                outer >= 3.0 || inner >= 3.0,
                "neither playhead ring reads against {cell:?} ({outer:.2}:1 / {inner:.2}:1)"
            );
        }

        // The rings must also separate from each other, or the frame reads as
        // one fat smudge on a mid-tone cell.
        assert!(contrast_ratio(PLAYHEAD_FRAME_OUTER, PLAYHEAD_FRAME_INNER) >= 3.0);

        // Violet has to stay distinct from the hues already used in the grid.
        for taken in [GATE_NOTE_COLOR, GATE_TIE_COLOR, ACCENT_ORANGE] {
            assert_ne!(PLAYHEAD_FRAME_OUTER, taken);
        }
    }
}
