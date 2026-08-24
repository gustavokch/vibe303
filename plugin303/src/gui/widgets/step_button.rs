use crate::gui::theme;
use crate::params::{SequencerStep, StepGate};
use nih_plug_egui::egui::{pos2, vec2, CornerRadius, FontFamily, FontId, Sense, Stroke, StrokeKind, Ui};

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
) -> bool {
    let mut changed = false;
    let width = 56.0;
    let is_active = is_current && is_running;

    ui.vertical(|ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);

        // 1. Playhead LED bar (4.0px flat bar at top, ruby accent distinct from Note/Tie/Acc)
        let (led_rect, led_resp) = ui.allocate_exact_size(vec2(width, 4.0), Sense::hover());
        let _ = led_resp.on_hover_text("Playhead position indicator");
        if ui.is_rect_visible(led_rect) {
            let p = ui.painter();
            let led_color = if is_active {
                theme::ACCENT_RUBY
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
        let (step_num_rect, _) = ui.allocate_exact_size(vec2(width, 16.0), Sense::hover());
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
                FontId::new(8.5, FontFamily::Monospace),
                text_color,
            );
        }

        // 3. Modifiers: ACC and SLD row (enlarged hit target >= 24px)
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
            let btn_w = (width - 2.0) * 0.5;

            // ACC Button (allocated 24.0px logical height for touch/hit target)
            let (acc_hit_rect, acc_resp) = ui.allocate_exact_size(vec2(btn_w, 24.0), Sense::click());
            let acc_resp = acc_resp.on_hover_text("Toggle Accent (A)");
            if acc_resp.clicked() {
                step.accent = !step.accent;
                changed = true;
            }
            if ui.is_rect_visible(acc_hit_rect) {
                let p = ui.painter();
                let acc_rect = nih_plug_egui::egui::Rect::from_center_size(acc_hit_rect.center(), vec2(btn_w, 18.0));
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
                    FontId::new(7.5, FontFamily::Proportional),
                    text_col,
                );
            }

            // SLD Button (allocated 24.0px logical height for touch/hit target)
            let (sld_hit_rect, sld_resp) = ui.allocate_exact_size(vec2(btn_w, 24.0), Sense::click());
            let sld_resp = sld_resp.on_hover_text("Toggle Slide (S)");
            if sld_resp.clicked() {
                step.slide = !step.slide;
                changed = true;
            }
            if ui.is_rect_visible(sld_hit_rect) {
                let p = ui.painter();
                let sld_rect = nih_plug_egui::egui::Rect::from_center_size(sld_hit_rect.center(), vec2(btn_w, 18.0));
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
                    FontId::new(7.5, FontFamily::Proportional),
                    text_col,
                );
            }
        });

        // 4. Gate Pill: NOTE / TIE / REST (allocated 24.0px logical height)
        let (gate_str, gate_bg, gate_text, gate_border) = match step.gate {
            StepGate::Note => ("NOTE", theme::ACCENT_CYAN, theme::TEXT_ON_ACCENT, theme::ACCENT_CYAN),
            StepGate::Tie => ("TIE", theme::ACCENT_GOLD, theme::TEXT_ON_ACCENT, theme::ACCENT_GOLD),
            StepGate::Rest => ("REST", theme::WELL_BG, theme::TEXT_MUTED, theme::BORDER_DEFAULT),
        };
        let (gate_hit_rect, gate_resp) = ui.allocate_exact_size(vec2(width, 24.0), Sense::click());
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
            let gate_rect = nih_plug_egui::egui::Rect::from_center_size(gate_hit_rect.center(), vec2(width, 18.0));
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
                FontId::new(8.0, FontFamily::Proportional),
                gate_text,
            );
        }

        // 5. Pitch Monospace Pill (allocated 24.0px logical height)
        let is_rest = step.gate == StepGate::Rest;
        let pitch_str = format_pitch(step.semitone, step.octave);
        let (pitch_hit_rect, pitch_resp) = ui.allocate_exact_size(vec2(width, 24.0), Sense::click_and_drag());
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
            let pitch_rect = nih_plug_egui::egui::Rect::from_center_size(pitch_hit_rect.center(), vec2(width, 18.0));
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
                FontId::new(8.5, FontFamily::Monospace),
                if is_rest { theme::TEXT_MUTED } else { theme::TEXT_PRIMARY },
            );
        }

        // 6. Octave Up / Down flat buttons (allocated >= 24px logical height for hit area)
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
            let oct_w = (width - 2.0) * 0.5;

            // Octave Up Button
            let (up_hit_rect, up_resp) = ui.allocate_exact_size(vec2(oct_w, 24.0), Sense::click());
            let up_resp = up_resp.on_hover_text("Octave up (+1)");
            if up_resp.clicked() && step.octave < 2 {
                step.octave += 1;
                changed = true;
            }
            if ui.is_rect_visible(up_hit_rect) {
                let p = ui.painter();
                let up_rect = nih_plug_egui::egui::Rect::from_center_size(up_hit_rect.center(), vec2(oct_w, 18.0));
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
                        pos2(cx, cy - 3.0),
                        pos2(cx - 3.0, cy + 2.0),
                        pos2(cx + 3.0, cy + 2.0),
                    ],
                    arrow_color,
                    Stroke::NONE,
                ));
            }

            // Octave Down Button
            let (dn_hit_rect, dn_resp) = ui.allocate_exact_size(vec2(oct_w, 24.0), Sense::click());
            let dn_resp = dn_resp.on_hover_text("Octave down (-1)");
            if dn_resp.clicked() && step.octave > -2 {
                step.octave -= 1;
                changed = true;
            }
            if ui.is_rect_visible(dn_hit_rect) {
                let p = ui.painter();
                let dn_rect = nih_plug_egui::egui::Rect::from_center_size(dn_hit_rect.center(), vec2(oct_w, 18.0));
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
                        pos2(cx - 3.0, cy - 2.0),
                        pos2(cx + 3.0, cy - 2.0),
                        pos2(cx, cy + 3.0),
                    ],
                    arrow_color,
                    Stroke::NONE,
                ));
            }
        });
    });

    changed
}
