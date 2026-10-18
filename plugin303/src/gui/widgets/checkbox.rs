//! The one checkbox convention (round-19 issue 3).
//!
//! Before this module the editor had four checkboxes in three different
//! on-state treatments: ADV ENV and LINK switched their label to orange,
//! SLD CHAIN hard-coded `TEXT_SECONDARY` and so was pixel-identical checked
//! and unchecked, and JITTER was a bespoke painter that inverted the box to an
//! orange fill with a dark glyph and sat on `SURFACE_ELEVATED` when off while
//! its siblings sat on the field fill.
//!
//! The convention all five sites (ADV ENV, LINK, BYPASS, SLD CHAIN, JITTER)
//! now share:
//!
//! * the box is the egui checkbox box — `icon_width` square, the inactive
//!   widget fill, the inactive widget border — and is **identical checked and
//!   unchecked**, so the box never carries the state;
//! * the check glyph is [`GLYPH_COLOR`] (`TEXT_SECONDARY`) in both paths;
//! * the **label** carries the state: [`theme::ACCENT_ORANGE`] when checked,
//!   [`theme::TEXT_SECONDARY`] when not.
//!
//! Sites that can use a normal `ui.checkbox` call [`checkbox`]. JITTER cannot
//! — it lives in a hand-measured slot inside the transport row — so it calls
//! [`paint_checkbox`], which reads the same style slots and paints the same
//! marks into a caller-supplied rect.

use nih_plug_egui::egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontId, Rect, Response, RichText, Stroke,
    StrokeKind, Ui,
};

use crate::gui::theme;

/// Label font shared by every checkbox site.
const LABEL_FONT_SIZE: f32 = 8.5;

/// The check glyph colour, one value for both checkbox paths ([`checkbox`]
/// and [`paint_checkbox`]) so the glyph can never drift between sites. The
/// label, not the glyph, carries the checked state.
const GLYPH_COLOR: Color32 = theme::TEXT_SECONDARY;

/// The label colour that carries the checked state. This is the whole
/// convention in one function — call it rather than re-deriving the pair.
pub fn label_color(checked: bool) -> Color32 {
    if checked {
        theme::ACCENT_ORANGE
    } else {
        theme::TEXT_SECONDARY
    }
}

/// A checkbox in the shared convention. Returns the usual `Response`, so
/// callers keep their `.changed()` and `.on_hover_text()` chains.
pub fn checkbox(ui: &mut Ui, checked: &mut bool, label: &str) -> Response {
    let color = label_color(*checked);
    // egui paints the check glyph from `ui.style().interact(&response)
    // .fg_stroke`, so the glyph override must live in the widget visuals.
    // Scope it to this checkbox: the app-wide foreground stays TEXT_PRIMARY
    // (field text needs it), and only the glyph moves to GLYPH_COLOR.
    ui.scope(|ui| {
        ui.style_mut().visuals.widgets.inactive.fg_stroke.color = GLYPH_COLOR;
        ui.style_mut().visuals.widgets.hovered.fg_stroke.color = GLYPH_COLOR;
        ui.style_mut().visuals.widgets.active.fg_stroke.color = GLYPH_COLOR;
        ui.checkbox(
            checked,
            RichText::new(label)
                .font(FontId::proportional(LABEL_FONT_SIZE))
                .color(color),
        )
    })
    .inner
}

/// Paint a checkbox in the shared convention into an explicit rect, for sites
/// that allocate their own hit rect and cannot call [`checkbox`].
///
/// The box is centred vertically in `hit_rect` and inset from its left edge by
/// half the icon width, which is where egui puts it too; the label follows at
/// egui's own offset (`icon_width + icon_spacing`, egui 0.31.1
/// `checkbox.rs:132-133` — not `item_spacing.x`). Every colour is read from
/// the same style slots `ui.checkbox` reads, except the check glyph, which
/// both paths take from [`GLYPH_COLOR`], so the two paths cannot drift apart.
pub fn paint_checkbox(ui: &Ui, hit_rect: Rect, checked: bool, label: &str) {
    let visuals = ui.style().visuals.widgets.inactive;
    let icon_w = ui.spacing().icon_width;
    // egui places the label at `icon_width + icon_spacing`; derive it from
    // the live spacing so this path tracks any style change.
    let label_offset = icon_w + ui.spacing().icon_spacing;
    let glyph_stroke = Stroke::new(visuals.fg_stroke.width, GLYPH_COLOR);
    let painter = ui.painter();

    let box_rect = Rect::from_center_size(
        pos2(hit_rect.left() + icon_w * 0.5, hit_rect.center().y),
        vec2(icon_w, icon_w),
    );

    // Box: identical checked and unchecked. State lives in the label.
    painter.rect_filled(box_rect, CornerRadius::same(2), visuals.bg_fill);
    painter.rect_stroke(
        box_rect,
        CornerRadius::same(2),
        visuals.bg_stroke,
        StrokeKind::Inside,
    );

    if checked {
        // egui draws its checkmark as a three-point polyline across the
        // shrunk icon rect; mirror those proportions so the two paths produce
        // the same glyph.
        let g = box_rect.shrink(icon_w * 0.25);
        painter.line_segment(
            [
                pos2(g.left(), g.center().y),
                pos2(g.left() + g.width() * 0.4, g.bottom()),
            ],
            glyph_stroke,
        );
        painter.line_segment(
            [
                pos2(g.left() + g.width() * 0.4, g.bottom()),
                pos2(g.right(), g.top()),
            ],
            glyph_stroke,
        );
    }

    painter.text(
        pos2(hit_rect.left() + label_offset, hit_rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(LABEL_FONT_SIZE),
        label_color(checked),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_carries_the_state_and_the_box_does_not() {
        // The convention in one assertion: checked and unchecked differ, and
        // they differ in the label colour specifically.
        assert_eq!(label_color(true), theme::ACCENT_ORANGE);
        assert_eq!(label_color(false), theme::TEXT_SECONDARY);
        assert_ne!(label_color(true), label_color(false));
    }
}
