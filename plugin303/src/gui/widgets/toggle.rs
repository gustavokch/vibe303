use nih_plug_egui::egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontFamily, FontId, Rect, Response, Sense, Ui,
    Widget,
};

use crate::gui::theme;

/// Default height of a toggle and flat button: one height, one row rhythm.
pub const TOGGLE_H: f32 = 18.0;

/// The state LED, a slot left of the label.
const LED_W: f32 = 3.0;
const LED_H: f32 = 10.0;
const LED_GAP: f32 = 5.0;
const PAD_X: f32 = 8.0;
/// The bar form of the LED: as tall as the step LED, inset from the tile edge.
const BAR_H: f32 = 4.0;
const BAR_INSET: f32 = 4.0;

/// A raised on/off tile with an LED: orange LED when on, an empty `WELL_BG`
/// slot when off. The tile itself never fills orange, so an off toggle still
/// reads as a toggle (its slot) and not as an action, and the lit area on
/// screen stays small (round-20 #4, #5). Replaces egui's checkbox.
pub struct ToggleButton<'a> {
    on: &'a mut bool,
    label: &'a str,
    min_width: f32,
    height: f32,
    led_bar: bool,
}

impl<'a> ToggleButton<'a> {
    pub fn new(on: &'a mut bool, label: &'a str) -> Self {
        Self {
            on,
            label,
            min_width: 0.0,
            height: TOGGLE_H,
            led_bar: false,
        }
    }

    /// Floor for the tile width, so a group of toggles can share one width.
    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    /// Tile height, for rows that run taller than the default.
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// Draw the LED as a bar across the bottom of the tile, the form the
    /// playhead's step LED takes, instead of the slot left of the label. For
    /// RUN only: the transport state must read from across the window, and a
    /// 3 x 10 pt slot does not (round-22 #9).
    pub fn led_bar(mut self) -> Self {
        self.led_bar = true;
        self
    }
}

/// Paint of one toggle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ToggleColors {
    fill: Color32,
    led: Color32,
    text: Color32,
}

fn toggle_colors(on: bool, hovered: bool, enabled: bool) -> ToggleColors {
    if !enabled {
        // A disabled tile sinks to the well plane: every disabled toggle sits
        // in a header strip, where SURFACE_PRESSED is the strip's own colour
        // (1.01:1, round-22 #10). The off slot merges into the tile.
        return ToggleColors {
            fill: theme::WELL_BG,
            led: if on {
                theme::TEXT_MUTED
            } else {
                theme::WELL_BG
            },
            text: theme::TEXT_MUTED,
        };
    }
    let led = match (on, hovered) {
        (true, true) => theme::ACCENT_ORANGE_HOVER,
        (true, false) => theme::ACCENT_ORANGE,
        (false, _) => theme::WELL_BG,
    };
    ToggleColors {
        fill: if hovered {
            theme::SURFACE_HOVER
        } else {
            theme::SURFACE_ELEVATED
        },
        led,
        text: if on || hovered {
            theme::TEXT_PRIMARY
        } else {
            theme::TEXT_SECONDARY
        },
    }
}

impl Widget for ToggleButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let font = FontId::new(8.5, FontFamily::Proportional);
        let text_w = ui.fonts(|f| {
            f.layout_no_wrap(self.label.to_string(), font.clone(), Color32::WHITE)
                .size()
                .x
        });
        let group_w = if self.led_bar {
            text_w
        } else {
            LED_W + LED_GAP + text_w
        };
        let width = (group_w + 2.0 * PAD_X).max(self.min_width);
        let (rect, mut response) = ui.allocate_exact_size(vec2(width, self.height), Sense::click());
        if response.clicked() {
            *self.on = !*self.on;
            response.mark_changed();
        }
        if ui.is_rect_visible(rect) {
            let mut painter = ui.painter().clone();
            painter.set_fade_to_color(None);
            let c = toggle_colors(*self.on, response.hovered(), ui.is_enabled());
            painter.rect_filled(rect, theme::radius(), c.fill);
            if self.led_bar {
                // The label centres in the space above the bar.
                let bar = Rect::from_min_max(
                    pos2(rect.left() + BAR_INSET, rect.bottom() - BAR_INSET - BAR_H),
                    pos2(rect.right() - BAR_INSET, rect.bottom() - BAR_INSET),
                );
                painter.rect_filled(bar, CornerRadius::same(1), c.led);
                painter.text(
                    pos2(rect.center().x, (rect.top() + bar.top()) * 0.5),
                    Align2::CENTER_CENTER,
                    self.label,
                    font,
                    c.text,
                );
            } else {
                // LED and label are centred as one group.
                let x0 = rect.center().x - group_w * 0.5;
                let led = Rect::from_center_size(
                    pos2(x0 + LED_W * 0.5, rect.center().y),
                    vec2(LED_W, LED_H),
                );
                painter.rect_filled(led, CornerRadius::same(1), c.led);
                painter.text(
                    pos2(x0 + LED_W + LED_GAP, rect.center().y),
                    Align2::LEFT_CENTER,
                    self.label,
                    font,
                    c.text,
                );
            }
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODES: [(bool, bool); 3] = [(false, true), (true, true), (false, false)]; // (hovered, enabled)

    /// A disabled toggle never shows the accent: lit orange means "live".
    #[test]
    fn disabled_toggle_never_claims_accent() {
        for on in [true, false] {
            for hovered in [true, false] {
                let c = toggle_colors(on, hovered, false);
                for accent in [theme::ACCENT_ORANGE, theme::ACCENT_ORANGE_HOVER] {
                    assert_ne!(c.fill, accent);
                    assert_ne!(c.led, accent);
                }
            }
        }
    }

    /// Only the LED carries the state: the tile is the same plane on and off,
    /// so a boolean never paints a block of orange (round-20 #4).
    #[test]
    fn only_the_led_shows_state() {
        for (hovered, enabled) in MODES {
            let on = toggle_colors(true, hovered, enabled);
            let off = toggle_colors(false, hovered, enabled);
            assert_eq!(on.fill, off.fill, "hovered={hovered} enabled={enabled}");
            assert_ne!(on.led, off.led, "hovered={hovered} enabled={enabled}");
        }
    }

    /// A lit LED is a non-text mark: ≥ 3:1 on its tile, disabled included, so a
    /// disabled selection still shows.
    #[test]
    fn lit_led_clears_the_mark_floor_on_its_tile() {
        for (hovered, enabled) in MODES {
            let c = toggle_colors(true, hovered, enabled);
            let ratio = theme::contrast_ratio(c.led, c.fill);
            assert!(
                ratio >= 3.0,
                "hovered={hovered} enabled={enabled}: {ratio:.2}"
            );
        }
    }

    /// A disabled tile stays visible on the header strip, where every disabled
    /// toggle sits: at least the quietest plane step the design uses (header
    /// strip on body). SURFACE_PRESSED there was 1.01:1 (round-22 #10).
    #[test]
    fn disabled_tile_shows_on_the_header_strip() {
        let floor = theme::contrast_ratio(theme::PANEL_HEADER_BG, theme::PANEL_BG);
        for on in [true, false] {
            let c = toggle_colors(on, false, false);
            let ratio = theme::contrast_ratio(c.fill, theme::PANEL_HEADER_BG);
            assert!(ratio >= floor, "on={on}: {ratio:.2} < {floor:.2}");
        }
    }
}
