use crate::gui::theme;
use crate::params::{SequencerStep, StepGate};
use nih_plug_egui::egui::{
    pos2, vec2, CornerRadius, FontFamily, FontId, Rect, Sense, Stroke, StrokeKind, Ui,
};

/// Geometry of one step strip, top to bottom. The painted column background is
/// drawn *before* the children (paint order within the layer decides stacking),
/// so its height cannot be measured after layout — it must be derived from the
/// same constants as the child allocations. Keep every allocation on these consts.
pub const LED_H: f32 = 4.0;
pub const HEADER_H: f32 = 16.0;
/// 28 px hit rows (24 px pills) keep the 2x8 grid inside the 960 px window
/// with bottom padding equal to the side margins (fix #1).
pub const CONTROL_ROW_H: f32 = 28.0;
pub const STRIP_HEIGHT: f32 = LED_H + HEADER_H + 4.0 * CONTROL_ROW_H;

/// Drawn pill height inside each 28 px hit row (4 px breathing room).
pub const PILL_H: f32 = CONTROL_ROW_H - 4.0;

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
    // Enhanced playhead: column glow when active
    let column_bg = if is_active {
        theme::SURFACE_ELEVATED
    } else if is_current {
        theme::WELL_BG
    } else {
        theme::PANEL_BG
    };

    ui.vertical(|ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let strip_min = ui.cursor().min;

        // Column background: painted before all children so within-layer paint
        // order keeps content on top, and painting never advances layout.
        let strip_rect = Rect::from_min_size(strip_min, vec2(width, cell_height));
        if ui.is_rect_visible(strip_rect) {
            ui.painter()
                .rect_filled(strip_rect, CornerRadius::same(2), column_bg);
        }
        // 1. Playhead LED bar (4.0px flat bar at top, ruby accent distinct from Note/Tie/Acc)
        let (led_rect, led_resp) = ui.allocate_exact_size(vec2(width, LED_H), Sense::hover());
        let _ = led_resp.on_hover_text("Playhead position indicator");
        if ui.is_rect_visible(led_rect) {
            let p = ui.painter();
            let led_color = if is_active {
                theme::ACCENT_CYAN
            } else {
                theme::WELL_BG
            };
            p.rect_filled(led_rect, CornerRadius::same(1), led_color);
            p.rect_stroke(
                led_rect,
                CornerRadius::same(1),
                Stroke::new(1.0_f32, if is_active { theme::ACCENT_RUBY } else { theme::BORDER_SUBTLE }),
                StrokeKind::Inside,
            );
        }

        // 2. Step number header pill
        let (step_num_rect, _) = ui.allocate_exact_size(vec2(width, HEADER_H), Sense::hover());
        if ui.is_rect_visible(step_num_rect) {
            let p = ui.painter();
            let (bg_color, text_color, stroke_color) = if is_active {
                (theme::SURFACE_ELEVATED, theme::ACCENT_RUBY, theme::ACCENT_RUBY)
            } else {
                (theme::WELL_BG, theme::TEXT_MUTED, theme::BORDER_DEFAULT)
            };
            p.rect_filled(step_num_rect, CornerRadius::same(2), bg_color);
            p.rect_stroke(
                step_num_rect,
                CornerRadius::same(2),
                Stroke::new(1.0_f32, stroke_color),
                StrokeKind::Inside,
            );
            p.text(
                step_num_rect.center(),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                format!("{:02}", step_idx + 1),
                FontId::new(10.0, FontFamily::Monospace),
                text_color,
            );
        }

        // 3. Modifiers: ACC and SLD row (enlarged hit target >= 24px)
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
            let btn_w = (width - 2.0) * 0.5;

            // ACC Button (30px hit target)
            let (acc_hit_rect, acc_resp) = ui.allocate_exact_size(vec2(btn_w, CONTROL_ROW_H), Sense::click());
            let acc_resp = acc_resp.on_hover_text("Toggle Accent (A)");
            if acc_resp.clicked() {
                step.accent = !step.accent;
                changed = true;
            }
            if ui.is_rect_visible(acc_hit_rect) {
                let p = ui.painter();
                let acc_rect = nih_plug_egui::egui::Rect::from_center_size(acc_hit_rect.center(), vec2(btn_w, PILL_H));
                let (bg, text_col) = if step.accent {
                    (theme::ACCENT_ORANGE, theme::TEXT_ON_ACCENT)
                } else if acc_resp.hovered() {
                    (theme::SURFACE_HOVER, theme::TEXT_SECONDARY)
                } else {
                    (theme::WELL_BG, theme::TEXT_MUTED)
                };
                p.rect_filled(acc_rect, CornerRadius::same(2), bg);
                p.rect_stroke(
                    acc_rect,
                    CornerRadius::same(2),
                    Stroke::new(1.0_f32, if step.accent { theme::ACCENT_ORANGE } else { theme::BORDER_DEFAULT }),
                    StrokeKind::Inside,
                );
                p.text(
                    acc_rect.center(),
                    nih_plug_egui::egui::Align2::CENTER_CENTER,
                    "ACC",
                    FontId::new(8.5, FontFamily::Proportional),
                    text_col,
                );
            }

            // SLD Button (30px hit target)
            let (sld_hit_rect, sld_resp) = ui.allocate_exact_size(vec2(btn_w, CONTROL_ROW_H), Sense::click());
            let sld_resp = sld_resp.on_hover_text("Toggle Slide (S)");
            if sld_resp.clicked() {
                step.slide = !step.slide;
                changed = true;
            }
            if ui.is_rect_visible(sld_hit_rect) {
                let p = ui.painter();
                let sld_rect = nih_plug_egui::egui::Rect::from_center_size(sld_hit_rect.center(), vec2(btn_w, PILL_H));
                let (bg, text_col) = if step.slide {
                    (theme::ACCENT_CYAN, theme::TEXT_ON_ACCENT)
                } else if sld_resp.hovered() {
                    (theme::SURFACE_HOVER, theme::TEXT_SECONDARY)
                } else {
                    (theme::WELL_BG, theme::TEXT_MUTED)
                };
                p.rect_filled(sld_rect, CornerRadius::same(2), bg);
                p.rect_stroke(
                    sld_rect,
                    CornerRadius::same(2),
                    Stroke::new(1.0_f32, if step.slide { theme::ACCENT_CYAN } else { theme::BORDER_DEFAULT }),
                    StrokeKind::Inside,
                );
                p.text(
                    sld_rect.center(),
                    nih_plug_egui::egui::Align2::CENTER_CENTER,
                    "SLD",
                    FontId::new(8.5, FontFamily::Proportional),
                    text_col,
                );
            }
        });

        // 4. Gate Pill: NOTE / TIE / REST (30px hit target)
        let (gate_str, gate_bg, gate_text, gate_border) = match step.gate {
            StepGate::Note => ("NOTE", theme::ACCENT_CYAN, theme::TEXT_ON_ACCENT, theme::ACCENT_CYAN),
            StepGate::Tie => ("TIE", theme::ACCENT_GOLD, theme::TEXT_ON_ACCENT, theme::ACCENT_GOLD),
            StepGate::Rest => ("REST", theme::WELL_BG, theme::TEXT_MUTED, theme::BORDER_DEFAULT),
        };
        let (gate_hit_rect, gate_resp) = ui.allocate_exact_size(vec2(width, CONTROL_ROW_H), Sense::click());
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
            let gate_rect = nih_plug_egui::egui::Rect::from_center_size(gate_hit_rect.center(), vec2(width, PILL_H));
            p.rect_filled(gate_rect, CornerRadius::same(2), gate_bg);
            p.rect_stroke(
                gate_rect,
                CornerRadius::same(2),
                Stroke::new(1.0_f32, gate_border),
                StrokeKind::Inside,
            );
            p.text(
                gate_rect.center(),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                gate_str,
                FontId::new(9.5, FontFamily::Proportional),
                gate_text,
            );
        }

        // 5. Pitch Monospace Pill (30px hit target)
        let is_rest = step.gate == StepGate::Rest;
        let pitch_str = format_pitch(step.semitone, step.octave);
        let (pitch_hit_rect, pitch_resp) = ui.allocate_exact_size(vec2(width, CONTROL_ROW_H), Sense::click_and_drag());
        let pitch_resp = pitch_resp.on_hover_text("Click: piano roll | Drag/R-Click: nudge pitch");

        let drag_id = ui.make_persistent_id(format!("step_drag_{}", step_idx));

        if pitch_resp.clicked() {
            crate::gui::widgets::piano_roll_popup::toggle_open_state(
                ui.ctx(),
                step_idx,
                pitch_resp.rect,
            );
        } else if pitch_resp.secondary_clicked() {
            step.semitone = if step.semitone <= 0 { 12 } else { step.semitone - 1 };
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
            let pitch_rect = nih_plug_egui::egui::Rect::from_center_size(pitch_hit_rect.center(), vec2(width, PILL_H));
            let bg = if pitch_resp.hovered() {
                theme::SURFACE_HOVER
            } else {
                theme::WELL_BG
            };
            p.rect_filled(pitch_rect, CornerRadius::same(2), bg);
            p.rect_stroke(
                pitch_rect,
                CornerRadius::same(2),
                Stroke::new(1.0_f32, if is_active && !is_rest { theme::ACCENT_RUBY } else { theme::BORDER_DEFAULT }),
                StrokeKind::Inside,
            );
            p.text(
                pitch_rect.center(),
                nih_plug_egui::egui::Align2::CENTER_CENTER,
                pitch_str,
                FontId::new(10.0, FontFamily::Monospace),
                if is_rest { theme::TEXT_MUTED } else { theme::TEXT_PRIMARY },
            );
        }

        // 6. Octave Up / Down flat buttons (30px hit target)
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
            let oct_w = (width - 2.0) * 0.5;

            // Octave Up Button
            let (up_hit_rect, up_resp) = ui.allocate_exact_size(vec2(oct_w, CONTROL_ROW_H), Sense::click());
            let up_resp = up_resp.on_hover_text("Octave up (+1)");
            if up_resp.clicked() && step.octave < 2 {
                step.octave += 1;
                changed = true;
            }
            if ui.is_rect_visible(up_hit_rect) {
                let p = ui.painter();
                let up_rect = nih_plug_egui::egui::Rect::from_center_size(up_hit_rect.center(), vec2(oct_w, PILL_H));
                let bg = if up_resp.hovered() && step.octave < 2 {
                    theme::SURFACE_HOVER
                } else {
                    theme::WELL_BG
                };
                p.rect_filled(up_rect, CornerRadius::same(2), bg);
                p.rect_stroke(
                    up_rect,
                    CornerRadius::same(2),
                    Stroke::new(1.0_f32, theme::BORDER_DEFAULT),
                    StrokeKind::Inside,
                );
                let cx = up_rect.center().x;
                let cy = up_rect.center().y;
                let arrow_color = if is_rest || step.octave >= 2 {
                    theme::TEXT_MUTED
                } else {
                    theme::TEXT_PRIMARY
                };
                p.add(nih_plug_egui::egui::Shape::convex_polygon(
                    vec![
                        pos2(cx, cy - 4.5),
                        pos2(cx - 4.5, cy + 3.0),
                        pos2(cx + 4.5, cy + 3.0),
                    ],
                    arrow_color,
                    Stroke::NONE,
                ));
            }

            // Octave Down Button
            let (dn_hit_rect, dn_resp) = ui.allocate_exact_size(vec2(oct_w, CONTROL_ROW_H), Sense::click());
            let dn_resp = dn_resp.on_hover_text("Octave down (-1)");
            if dn_resp.clicked() && step.octave > -2 {
                step.octave -= 1;
                changed = true;
            }
            if ui.is_rect_visible(dn_hit_rect) {
                let p = ui.painter();
                let dn_rect = nih_plug_egui::egui::Rect::from_center_size(dn_hit_rect.center(), vec2(oct_w, PILL_H));
                let bg = if dn_resp.hovered() && step.octave > -2 {
                    theme::SURFACE_HOVER
                } else {
                    theme::WELL_BG
                };
                p.rect_filled(dn_rect, CornerRadius::same(2), bg);
                p.rect_stroke(
                    dn_rect,
                    CornerRadius::same(2),
                    Stroke::new(1.0_f32, theme::BORDER_DEFAULT),
                    StrokeKind::Inside,
                );
                let cx = dn_rect.center().x;
                let cy = dn_rect.center().y;
                let arrow_color = if is_rest || step.octave <= -2 {
                    theme::TEXT_MUTED
                } else {
                    theme::TEXT_PRIMARY
                };
                p.add(nih_plug_egui::egui::Shape::convex_polygon(
                    vec![
                        pos2(cx - 4.5, cy - 3.0),
                        pos2(cx + 4.5, cy - 3.0),
                        pos2(cx, cy + 4.5),
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

    #[test]
    fn strip_height_equals_sum_of_child_allocations() {
        // LED bar + step-number header + ACC/SLD row + gate + pitch + octave row
        assert_eq!(LED_H, 4.0);
        assert_eq!(HEADER_H, 16.0);
        assert_eq!(CONTROL_ROW_H, 28.0);
        let children_sum = LED_H + HEADER_H + 4.0 * CONTROL_ROW_H;
        assert_eq!(STRIP_HEIGHT, children_sum);
    }

    #[test]
    fn drawn_pill_fits_inside_hit_row() {
        assert!(PILL_H < CONTROL_ROW_H);
    }
}
