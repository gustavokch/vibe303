use crate::gui::theme;
use crate::params::{SequencerStep, StepGate};
use nih_plug_egui::egui::{
    pos2, vec2, Color32, CornerRadius, FontFamily, FontId, Rect, Sense, Stroke, Ui,
};

/// Geometry of one step strip, top to bottom. The painted column background is
/// drawn *before* the children (paint order within the layer decides stacking),
/// so its height cannot be measured after layout — it must be derived from the
/// same constants as the child allocations. Keep every allocation on these consts.
pub const LED_H: f32 = 4.0;
pub const HEADER_H: f32 = 11.0;
/// 21 px hit rows (19 px pills with 2 px vertical gap) for the compact 1216x640 layout.
pub const CONTROL_ROW_H: f32 = 21.0;
pub const STRIP_HEIGHT: f32 = LED_H + HEADER_H + 4.0 * CONTROL_ROW_H;

/// Drawn pill height inside each 21 px hit row (2 px vertical gap between adjacent pills).
pub const PILL_H: f32 = CONTROL_ROW_H - 2.0;

/// Hover text of a step's ACC flag. The flags have no keyboard shortcut.
const ACC_HINT: &str = "Lit: this step is accented (louder, with a stronger filter sweep).";
/// Hover text of a step's SLD flag.
const SLD_HINT: &str = "Lit: this note slides into the next one (held gate, pitch glide).";

/// Fill and text of an ACC or SLD flag. A lit flag lifts to its hover fill
/// under the pointer like any lit tile; it stayed flat before (round-22 #18).
fn flag_colors(
    lit: bool,
    hovered: bool,
    lit_fill: Color32,
    lit_hover: Color32,
) -> (Color32, Color32) {
    match (lit, hovered) {
        (true, false) => (lit_fill, theme::TEXT_ON_ACCENT),
        (true, true) => (lit_hover, theme::TEXT_ON_ACCENT),
        (false, true) => (theme::SURFACE_HOVER, theme::TEXT_SECONDARY),
        (false, false) => (theme::SURFACE_ELEVATED, theme::TEXT_SECONDARY),
    }
}

/// Paint one flag pill, `PILL_H` tall, centred in its hit row.
fn paint_flag(ui: &Ui, hit_rect: Rect, label: &str, fill: Color32, text: Color32) {
    let pill = Rect::from_center_size(hit_rect.center(), vec2(hit_rect.width(), PILL_H));
    let p = ui.painter();
    p.rect_filled(pill, theme::radius(), fill);
    p.text(
        pill.center(),
        nih_plug_egui::egui::Align2::CENTER_CENTER,
        label,
        FontId::new(8.5, FontFamily::Proportional),
        text,
    );
}

pub fn format_pitch(semitone: i8, octave: i8) -> String {
    let note_names = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B", "C",
    ];
    let name = note_names
        .get(semitone.clamp(0, 12) as usize)
        .unwrap_or(&"C");
    let oct_num = 3 + octave + if semitone == 12 { 1 } else { 0 };
    format!("{} {}", name, oct_num)
}

pub fn render_step_strip(
    ui: &mut Ui,
    step_idx: usize,
    step: &mut SequencerStep,
    is_current: bool,
    is_running: bool,
    cell_width: f32,
    cell_height: f32,
) -> bool {
    let mut changed = false;
    let width = cell_width;
    let is_active = is_current && is_running;
    ui.vertical(|ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let strip_min = ui.cursor().min;

        // The plate marks the playing step only: while stopped no step plays,
        // so none is marked (round-20 #6).
        let strip_rect = Rect::from_min_size(strip_min, vec2(width, cell_height));
        if is_active && ui.is_rect_visible(strip_rect) {
            ui.painter()
                .rect_filled(strip_rect, theme::radius(), theme::PANEL_HEADER_BG);
        }
        // 1. Step LED: a LED_H bar across the column; ACCENT_ORANGE on the playing step, WELL_BG otherwise.
        let (led_rect, led_resp) = ui.allocate_exact_size(vec2(width, LED_H), Sense::hover());
        let _ = led_resp.on_hover_text("Lit: the sequencer is playing this step.");
        if ui.is_rect_visible(led_rect) {
            let p = ui.painter();
            let led_color = if is_active {
                theme::ACCENT_ORANGE
            } else {
                theme::WELL_BG
            };
            p.rect_filled(led_rect, CornerRadius::same(1), led_color);
        }

        // 2. Step number header text (monospace; TEXT_PRIMARY when playing, TEXT_SECONDARY otherwise)
        let (step_num_rect, _) = ui.allocate_exact_size(vec2(width, HEADER_H), Sense::hover());
        if ui.is_rect_visible(step_num_rect) {
            let p = ui.painter();
            let num_color = if is_active {
                theme::TEXT_PRIMARY
            } else {
                theme::TEXT_SECONDARY
            };
            p.text(
                step_num_rect.center(),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                format!("{:02}", step_idx + 1),
                FontId::new(8.5, FontFamily::Monospace),
                num_color,
            );
        }

        // 3. Modifiers: ACC and SLD row (21 px hit row)
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
            let btn_w = (width - 2.0) * 0.5;

            // ACC Button
            let (acc_hit_rect, acc_resp) =
                ui.allocate_exact_size(vec2(btn_w, CONTROL_ROW_H), Sense::click());
            let acc_resp = acc_resp.on_hover_text(ACC_HINT);
            if acc_resp.clicked() {
                step.accent = !step.accent;
                changed = true;
            }
            if ui.is_rect_visible(acc_hit_rect) {
                let (bg, text_col) = flag_colors(
                    step.accent,
                    acc_resp.hovered(),
                    theme::ACCENT_ORANGE,
                    theme::ACCENT_ORANGE_HOVER,
                );
                paint_flag(ui, acc_hit_rect, "ACC", bg, text_col);
            }

            // SLD Button
            let (sld_hit_rect, sld_resp) =
                ui.allocate_exact_size(vec2(btn_w, CONTROL_ROW_H), Sense::click());
            let sld_resp = sld_resp.on_hover_text(SLD_HINT);
            if sld_resp.clicked() {
                step.slide = !step.slide;
                changed = true;
            }
            if ui.is_rect_visible(sld_hit_rect) {
                let (bg, text_col) = flag_colors(
                    step.slide,
                    sld_resp.hovered(),
                    theme::ACCENT_BLUE,
                    theme::ACCENT_BLUE_HOVER,
                );
                paint_flag(ui, sld_hit_rect, "SLD", bg, text_col);
            }
        });

        let is_rest = matches!(step.gate, StepGate::Rest);

        // 4. Gate Pill: NOTE / TIE / REST (21 px hit row)
        let (gate_str, gate_bg, gate_text) = match step.gate {
            StepGate::Note => ("NOTE", theme::GATE_NOTE_FILL, theme::TEXT_ON_ACCENT),
            StepGate::Tie => ("TIE", theme::GATE_TIE_FILL, theme::TEXT_PRIMARY),
            StepGate::Rest => ("REST", theme::WELL_BG, theme::TEXT_MUTED),
        };
        let (gate_hit_rect, gate_resp) =
            ui.allocate_exact_size(vec2(width, CONTROL_ROW_H), Sense::click());
        if gate_resp.clicked() {
            step.gate = match step.gate {
                StepGate::Note => StepGate::Tie,
                StepGate::Tie => StepGate::Rest,
                StepGate::Rest => StepGate::Note,
            };
            changed = true;
        }
        if ui.is_rect_visible(gate_hit_rect) {
            let p = ui.painter();
            let gate_rect = nih_plug_egui::egui::Rect::from_center_size(
                gate_hit_rect.center(),
                vec2(width, PILL_H),
            );
            p.rect_filled(gate_rect, theme::radius(), gate_bg);
            p.text(
                gate_rect.center(),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                gate_str,
                FontId::new(8.5, FontFamily::Proportional),
                gate_text,
            );
        }

        // 5. Pitch Monospace Pill (21 px hit row)
        let pitch_str = format_pitch(step.semitone, step.octave);
        let (pitch_hit_rect, pitch_resp) =
            ui.allocate_exact_size(vec2(width, CONTROL_ROW_H), Sense::click_and_drag());
        let pitch_resp = pitch_resp.on_hover_text("Click: piano roll | Drag/R-Click: nudge pitch");

        let drag_id = ui.make_persistent_id(nih_plug_egui::egui::Id::new(("step_drag", step_idx)));

        if pitch_resp.clicked() {
            crate::gui::widgets::piano_roll_popup::toggle_open_state(
                ui.ctx(),
                step_idx,
                pitch_resp.rect,
            );
        } else if pitch_resp.secondary_clicked() {
            step.semitone = if step.semitone <= 0 {
                12
            } else {
                step.semitone - 1
            };
            changed = true;
        } else if pitch_resp.dragged() {
            let dy = -pitch_resp.drag_delta().y;
            let mut accum: f32 = ui.ctx().data(|d| d.get_temp(drag_id)).unwrap_or(0.0);
            accum += dy;

            const PIXELS_PER_SEMITONE: f32 = 6.0;
            if accum.abs() >= PIXELS_PER_SEMITONE {
                let steps = (accum / PIXELS_PER_SEMITONE).trunc() as i8;
                if steps != 0 {
                    step.semitone = (step.semitone + steps).clamp(0, 12);
                    accum -= steps as f32 * PIXELS_PER_SEMITONE;
                    changed = true;
                }
            }
            ui.ctx().data_mut(|d| d.insert_temp(drag_id, accum));
        }

        if pitch_resp.drag_stopped() {
            ui.ctx().data_mut(|d| d.remove_temp::<f32>(drag_id));
        }

        if ui.is_rect_visible(pitch_hit_rect) {
            let p = ui.painter();
            let pitch_rect = nih_plug_egui::egui::Rect::from_center_size(
                pitch_hit_rect.center(),
                vec2(width, PILL_H),
            );
            let bg = if pitch_resp.hovered() {
                theme::SURFACE_PRESSED
            } else {
                theme::WELL_BG
            };
            p.rect_filled(pitch_rect, theme::radius(), bg);
            // REST steps render no pitch text (round-18 issue 14)
            if !is_rest {
                p.text(
                    pitch_rect.center(),
                    nih_plug_egui::egui::Align2::CENTER_CENTER,
                    pitch_str,
                    FontId::new(8.5, FontFamily::Monospace),
                    theme::TEXT_PRIMARY,
                );
            }
        }

        // 6. Octave Up / Down flat buttons (21 px hit row)
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
            let oct_w = (width - 2.0) * 0.5;
            // Octave Up Button
            let (up_hit_rect, up_resp) =
                ui.allocate_exact_size(vec2(oct_w, CONTROL_ROW_H), Sense::click());
            let up_resp = up_resp.on_hover_text("Octave up (+1)");
            if up_resp.clicked() && step.octave < 2 {
                step.octave += 1;
                changed = true;
            }
            if ui.is_rect_visible(up_hit_rect) {
                let p = ui.painter();
                let up_rect = nih_plug_egui::egui::Rect::from_center_size(
                    up_hit_rect.center(),
                    vec2(oct_w, PILL_H),
                );
                let bg = if up_resp.hovered() && step.octave < 2 {
                    theme::SURFACE_HOVER
                } else {
                    theme::SURFACE_ELEVATED
                };
                p.rect_filled(up_rect, theme::radius(), bg);
                let cx = up_rect.center().x;
                let cy = up_rect.center().y;
                let arrow_color = if is_rest || step.octave >= 2 {
                    theme::TEXT_MUTED
                } else {
                    theme::TEXT_PRIMARY
                };
                p.add(nih_plug_egui::egui::Shape::convex_polygon(
                    vec![
                        pos2(cx, cy - 3.5),
                        pos2(cx - 3.5, cy + 2.5),
                        pos2(cx + 3.5, cy + 2.5),
                    ],
                    arrow_color,
                    Stroke::NONE,
                ));
            }

            // Octave Down Button
            let (dn_hit_rect, dn_resp) =
                ui.allocate_exact_size(vec2(oct_w, CONTROL_ROW_H), Sense::click());
            let dn_resp = dn_resp.on_hover_text("Octave down (-1)");
            if dn_resp.clicked() && step.octave > -2 {
                step.octave -= 1;
                changed = true;
            }
            if ui.is_rect_visible(dn_hit_rect) {
                let p = ui.painter();
                let dn_rect = nih_plug_egui::egui::Rect::from_center_size(
                    dn_hit_rect.center(),
                    vec2(oct_w, PILL_H),
                );
                let bg = if dn_resp.hovered() && step.octave > -2 {
                    theme::SURFACE_HOVER
                } else {
                    theme::SURFACE_ELEVATED
                };
                p.rect_filled(dn_rect, theme::radius(), bg);
                let cx = dn_rect.center().x;
                let cy = dn_rect.center().y;
                let arrow_color = if is_rest || step.octave <= -2 {
                    theme::TEXT_MUTED
                } else {
                    theme::TEXT_PRIMARY
                };
                p.add(nih_plug_egui::egui::Shape::convex_polygon(
                    vec![
                        pos2(cx - 3.5, cy - 2.5),
                        pos2(cx + 3.5, cy - 2.5),
                        pos2(cx, cy + 3.5),
                    ],
                    arrow_color,
                    Stroke::NONE,
                ));
            }
        });
    });

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every flag state answers the pointer: lit or unlit, ACC or SLD, the
    /// hovered fill differs from the resting one, and the label keeps its
    /// contrast on both.
    #[test]
    fn every_flag_state_shows_hover() {
        for (lit_fill, lit_hover) in [
            (theme::ACCENT_ORANGE, theme::ACCENT_ORANGE_HOVER),
            (theme::ACCENT_BLUE, theme::ACCENT_BLUE_HOVER),
        ] {
            for lit in [true, false] {
                let rest = flag_colors(lit, false, lit_fill, lit_hover);
                let hover = flag_colors(lit, true, lit_fill, lit_hover);
                assert_ne!(rest.0, hover.0, "lit={lit} {lit_fill:?}");
                for (fill, text) in [rest, hover] {
                    let ratio = theme::contrast_ratio(text, fill);
                    assert!(ratio >= 4.5, "lit={lit} {fill:?}: {ratio:.2}");
                }
            }
        }
    }
}
