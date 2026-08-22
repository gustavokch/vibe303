use crate::gui::theme;
use crate::params::{SequencerStep, StepGate};
use nih_plug_egui::egui::{pos2, vec2, CornerRadius, FontFamily, FontId, Rect, Sense, Stroke, StrokeKind, Ui};

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

    let card_height = 148.0;
    let (card_rect, _) = ui.allocate_exact_size(vec2(width, card_height), Sense::hover());

    if ui.is_rect_visible(card_rect) {
        let p = ui.painter();

        // 0. Card container frame
        let border_color = if is_active {
            theme::ACCENT_ORANGE
        } else {
            theme::BORDER_DEFAULT
        };
        p.rect_filled(card_rect, CornerRadius::same(3), theme::PANEL_BG);
        p.rect_stroke(
            card_rect,
            CornerRadius::same(3),
            Stroke::new(1.0_f32, border_color),
            StrokeKind::Inside,
        );

        // 1. Playhead LED bar (4.0px flat bar at top)
        let led_rect = Rect::from_min_size(
            pos2(card_rect.left() + 4.0, card_rect.top() + 4.0),
            vec2(width - 8.0, 4.0),
        );
        let led_color = if is_active {
            theme::ACCENT_ORANGE
        } else {
            theme::SURFACE_ELEVATED
        };
        p.rect_filled(led_rect, CornerRadius::same(1), led_color);
    }

    // Allocate child widgets inside card
    let child_rect = Rect::from_min_size(
        pos2(card_rect.left() + 4.0, card_rect.top() + 11.0),
        vec2(width - 8.0, card_height - 15.0),
    );
    let mut child_ui = ui.new_child(nih_plug_egui::egui::UiBuilder::new().max_rect(child_rect));

    let child = &mut child_ui;
    child.set_width(width - 8.0);
    child.spacing_mut().item_spacing = vec2(0.0, 3.0);

    // Step number label
    child.horizontal(|ui| {
        ui.set_width(width - 8.0);
        ui.centered_and_justified(|ui| {
            ui.label(
                nih_plug_egui::egui::RichText::new(format!("{:02}", step_idx + 1))
                    .font(FontId::new(9.0, FontFamily::Monospace))
                    .color(if is_active {
                        theme::ACCENT_ORANGE
                    } else {
                        theme::TEXT_MUTED
                    }),
            );
        });
    });

    // 2. Modifiers: ACC and SLD row
    child.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
        let btn_w = (width - 11.0) * 0.5;

        // ACC Button
        let (acc_rect, acc_resp) = ui.allocate_exact_size(vec2(btn_w, 16.0), Sense::click());
        let acc_resp = acc_resp.on_hover_text("Toggle Accent (A)");
        if acc_resp.clicked() {
            step.accent = !step.accent;
            changed = true;
        }
        if ui.is_rect_visible(acc_rect) {
            let p = ui.painter();
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

        // SLD Button
        let (sld_rect, sld_resp) = ui.allocate_exact_size(vec2(btn_w, 16.0), Sense::click());
        let sld_resp = sld_resp.on_hover_text("Toggle Slide (S)");
        if sld_resp.clicked() {
            step.slide = !step.slide;
            changed = true;
        }
        if ui.is_rect_visible(sld_rect) {
            let p = ui.painter();
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

    // 3. Gate Pill: NOTE / TIE / REST
    let (gate_str, gate_bg, gate_text, gate_border) = match step.gate {
        StepGate::Note => ("NOTE", theme::ACCENT_CYAN, theme::TEXT_ON_ACCENT, theme::ACCENT_CYAN),
        StepGate::Tie => ("TIE", theme::ACCENT_GOLD, theme::TEXT_ON_ACCENT, theme::ACCENT_GOLD),
        StepGate::Rest => ("REST", theme::WELL_BG, theme::TEXT_MUTED, theme::BORDER_DEFAULT),
    };
    let (gate_rect, gate_resp) = child.allocate_exact_size(vec2(width - 8.0, 18.0), Sense::click());
    if gate_resp.clicked() {
        step.gate = match step.gate {
            StepGate::Note => StepGate::Tie,
            StepGate::Tie => StepGate::Rest,
            StepGate::Rest => StepGate::Note,
        };
        changed = true;
    }
    if child.is_rect_visible(gate_rect) {
        let p = child.painter();
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
            FontId::new(8.5, FontFamily::Proportional),
            gate_text,
        );
    }

    // 4. Pitch Monospace Pill (de-emphasized on REST)
    let is_rest = step.gate == StepGate::Rest;
    let pitch_str = format_pitch(step.semitone, step.octave);
    let (pitch_rect, pitch_resp) = child.allocate_exact_size(vec2(width - 8.0, 20.0), Sense::click_and_drag());
    let pitch_resp = pitch_resp.on_hover_text("Click: piano roll | Drag/R-Click: nudge pitch");

    if pitch_resp.clicked() {
        crate::gui::widgets::piano_roll_popup::toggle_open_state(
            child.ctx(),
            step_idx,
            pitch_resp.rect,
        );
    } else if pitch_resp.secondary_clicked() {
        step.semitone = if step.semitone <= 0 { 12 } else { step.semitone - 1 };
        changed = true;
    } else if pitch_resp.dragged() {
        let dy = -pitch_resp.drag_delta().y;
        if dy.abs() >= 4.0 {
            let steps = (dy / 4.0).round() as i8;
            step.semitone = (step.semitone + steps).clamp(0, 12);
            changed = true;
        }
    }

    if child.is_rect_visible(pitch_rect) {
        let p = child.painter();
        let bg = if pitch_resp.hovered() {
            theme::SURFACE_HOVER
        } else {
            theme::WELL_BG
        };
        p.rect_filled(pitch_rect, CornerRadius::same(2), bg);
        p.rect_stroke(
            pitch_rect,
            CornerRadius::same(2),
            Stroke::new(1.0_f32, theme::BORDER_DEFAULT),
            StrokeKind::Inside,
        );
        p.text(
            pitch_rect.center(),
            nih_plug_egui::egui::Align2::CENTER_CENTER,
            pitch_str,
            FontId::new(9.5, FontFamily::Monospace),
            if is_rest { theme::TEXT_MUTED } else { theme::TEXT_PRIMARY },
        );
    }

    // 5. Octave Up / Down flat buttons
    child.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
        let oct_w = (width - 11.0) * 0.5;

        // Octave Up Button
        let (up_rect, up_resp) = ui.allocate_exact_size(vec2(oct_w, 16.0), Sense::click());
        let up_resp = up_resp.on_hover_text("Octave up (+1)");
        if up_resp.clicked() && step.octave < 2 {
            step.octave += 1;
            changed = true;
        }
        if ui.is_rect_visible(up_rect) {
            let p = ui.painter();
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
                    pos2(cx, cy - 3.5),
                    pos2(cx - 3.5, cy + 2.5),
                    pos2(cx + 3.5, cy + 2.5),
                ],
                arrow_color,
                Stroke::NONE,
            ));
        }

        // Octave Down Button
        let (dn_rect, dn_resp) = ui.allocate_exact_size(vec2(oct_w, 16.0), Sense::click());
        let dn_resp = dn_resp.on_hover_text("Octave down (-1)");
        if dn_resp.clicked() && step.octave > -2 {
            step.octave -= 1;
            changed = true;
        }
        if ui.is_rect_visible(dn_rect) {
            let p = ui.painter();
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
                    pos2(cx - 3.5, cy - 2.5),
                    pos2(cx + 3.5, cy - 2.5),
                    pos2(cx, cy + 3.5),
                ],
                arrow_color,
                Stroke::NONE,
            ));
        }
    });

    changed
}
