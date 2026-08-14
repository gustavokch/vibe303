use crate::gui::theme;
use crate::params::{SequencerStep, StepGate};
use nih_plug_egui::egui::{pos2, vec2, Color32, FontFamily, FontId, Sense, Stroke, Ui};

pub fn format_pitch(semitone: i8, octave: i8) -> String {
    let note_names = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B", "C",
    ];
    let name = note_names
        .get(semitone.clamp(0, 12) as usize)
        .unwrap_or(&"C");
    let oct_num = 3 + octave;
    format!("{} {}", name, oct_num)
}

pub fn render_step_strip(
    ui: &mut Ui,
    step_idx: usize,
    step: &mut SequencerStep,
    is_active: bool,
) -> bool {
    let mut changed = false;
    let width = 36.0;

    ui.vertical(|ui| {
        ui.set_width(width);

        // 1. Playhead LED bar
        let (led_rect, _) = ui.allocate_exact_size(vec2(width, 5.0), Sense::hover());
        if is_active {
            ui.painter().rect_filled(led_rect, 2.0, theme::PLAYHEAD_RED);
            ui.painter()
                .rect_filled(led_rect.expand(2.0), 3.0, theme::PLAYHEAD_RED_GLOW);
        } else {
            ui.painter()
                .rect_filled(led_rect, 2.0, Color32::from_rgb(0x22, 0x26, 0x2e));
        }

        ui.add_space(2.0);

        // Step number label
        ui.horizontal(|ui| {
            ui.set_width(width);
            ui.centered_and_justified(|ui| {
                ui.label(
                    nih_plug_egui::egui::RichText::new(format!("{:02}", step_idx + 1))
                        .font(FontId::new(9.0, FontFamily::Monospace))
                        .color(if is_active {
                            theme::TEXT_PRIMARY
                        } else {
                            theme::TEXT_MUTED
                        }),
                );
            });
        });

        ui.add_space(2.0);

        // 2. Modifiers: ACC and SLD with discrete LED dots
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 0.0);

            // ACC Button
            let (acc_rect, acc_resp) =
                ui.allocate_exact_size(vec2(17.0, 16.0), Sense::click());
            let acc_resp = acc_resp.on_hover_text("Toggle Accent (A)");
            if acc_resp.clicked() {
                step.accent = !step.accent;
                changed = true;
            }
            if ui.is_rect_visible(acc_rect) {
                let p = ui.painter();
                p.rect_filled(
                    acc_rect,
                    2.0,
                    if acc_resp.hovered() {
                        Color32::from_rgb(0x32, 0x38, 0x44)
                    } else {
                        Color32::from_rgb(0x20, 0x24, 0x2c)
                    },
                );
                p.rect_stroke(
                    acc_rect,
                    2.0,
                    Stroke::new(1.0_f32, theme::PANEL_BORDER),
                    nih_plug_egui::egui::StrokeKind::Inside,
                );
                p.text(
                    pos2(acc_rect.left() + 2.0, acc_rect.center().y),
                    nih_plug_egui::egui::Align2::LEFT_CENTER,
                    "ACC",
                    FontId::new(6.0, FontFamily::Proportional),
                    theme::TEXT_PRIMARY,
                );
                let led_pos = pos2(acc_rect.right() - 3.0, acc_rect.center().y);
                if step.accent {
                    p.circle_filled(led_pos, 2.5, theme::ACCENT_AMBER_GLOW);
                    p.circle_filled(led_pos, 1.5, theme::ACCENT_AMBER);
                } else {
                    p.circle_filled(led_pos, 1.3, Color32::from_rgb(0x14, 0x16, 0x1c));
                }
            }

            // SLD Button
            let (sld_rect, sld_resp) =
                ui.allocate_exact_size(vec2(17.0, 16.0), Sense::click());
            let sld_resp = sld_resp.on_hover_text("Toggle Slide (S)");
            if sld_resp.clicked() {
                step.slide = !step.slide;
                changed = true;
            }
            if ui.is_rect_visible(sld_rect) {
                let p = ui.painter();
                p.rect_filled(
                    sld_rect,
                    2.0,
                    if sld_resp.hovered() {
                        Color32::from_rgb(0x32, 0x38, 0x44)
                    } else {
                        Color32::from_rgb(0x20, 0x24, 0x2c)
                    },
                );
                p.rect_stroke(
                    sld_rect,
                    2.0,
                    Stroke::new(1.0_f32, theme::PANEL_BORDER),
                    nih_plug_egui::egui::StrokeKind::Inside,
                );
                p.text(
                    pos2(sld_rect.left() + 2.0, sld_rect.center().y),
                    nih_plug_egui::egui::Align2::LEFT_CENTER,
                    "SLD",
                    FontId::new(6.0, FontFamily::Proportional),
                    theme::TEXT_PRIMARY,
                );
                let led_pos = pos2(sld_rect.right() - 3.0, sld_rect.center().y);
                if step.slide {
                    p.circle_filled(led_pos, 2.5, theme::SLIDE_CYAN_GLOW);
                    p.circle_filled(led_pos, 1.5, theme::SLIDE_CYAN);
                } else {
                    p.circle_filled(led_pos, 1.3, Color32::from_rgb(0x14, 0x16, 0x1c));
                }
            }
        });

        ui.add_space(2.0);

        // 3. Gate Badge: Note / Tie / Rest (High contrast)
        let (gate_str, gate_color, text_color) = match step.gate {
            StepGate::Note => ("NOTE", theme::GATE_NOTE_COLOR, Color32::BLACK),
            StepGate::Tie => ("TIE", theme::GATE_TIE_COLOR, Color32::BLACK),
            StepGate::Rest => (
                "REST",
                theme::GATE_REST_COLOR,
                Color32::from_rgb(0xee, 0xf2, 0xf8),
            ),
        };
        let gate_btn = nih_plug_egui::egui::Button::new(
            nih_plug_egui::egui::RichText::new(gate_str)
                .font(FontId::new(8.0, FontFamily::Proportional))
                .strong()
                .color(text_color),
        )
        .fill(gate_color)
        .min_size(vec2(width, 18.0));

        if ui.add(gate_btn).clicked() {
            step.gate = match step.gate {
                StepGate::Note => StepGate::Tie,
                StepGate::Tie => StepGate::Rest,
                StepGate::Rest => StepGate::Note,
            };
            changed = true;
        }

        ui.add_space(2.0);

        // 4. Pitch & Octave
        let pitch_str = format_pitch(step.semitone, step.octave);
        let pitch_btn = nih_plug_egui::egui::Button::new(
            nih_plug_egui::egui::RichText::new(pitch_str)
                .font(FontId::new(9.0, FontFamily::Monospace))
                .strong()
                .color(theme::TEXT_PRIMARY),
        )
        .fill(Color32::from_rgb(0x18, 0x1c, 0x24))
        .min_size(vec2(width, 18.0));

        let pitch_resp = ui
            .add(pitch_btn)
            .on_hover_text("Click to open piano roll (or drag/right-click to nudge)");
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
            if dy.abs() >= 4.0 {
                let steps = (dy / 4.0).round() as i8;
                step.semitone = (step.semitone + steps).clamp(0, 12);
                changed = true;
            }
        }

        ui.add_space(2.0);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 0.0);

            // Octave Up Button
            let (up_rect, up_resp) = ui.allocate_exact_size(vec2(17.0, 16.0), Sense::click());
            let up_resp = up_resp.on_hover_text("Octave up (+1)");
            if up_resp.clicked() && step.octave < 2 {
                step.octave += 1;
                changed = true;
            }
            if ui.is_rect_visible(up_rect) {
                let p = ui.painter();
                p.rect_filled(
                    up_rect,
                    2.0,
                    if up_resp.hovered() {
                        Color32::from_rgb(0x32, 0x38, 0x44)
                    } else {
                        Color32::from_rgb(0x20, 0x24, 0x2c)
                    },
                );
                p.rect_stroke(
                    up_rect,
                    2.0,
                    Stroke::new(1.0_f32, theme::PANEL_BORDER),
                    nih_plug_egui::egui::StrokeKind::Inside,
                );
                let cx = up_rect.center().x;
                let cy = up_rect.center().y;
                let arrow_color = if step.octave < 2 {
                    theme::TEXT_PRIMARY
                } else {
                    theme::TEXT_MUTED
                };
                p.line_segment(
                    [pos2(cx - 2.5, cy + 1.5), pos2(cx, cy - 2.0)],
                    Stroke::new(1.3_f32, arrow_color),
                );
                p.line_segment(
                    [pos2(cx, cy - 2.0), pos2(cx + 2.5, cy + 1.5)],
                    Stroke::new(1.3_f32, arrow_color),
                );
            }

            // Octave Down Button
            let (dn_rect, dn_resp) = ui.allocate_exact_size(vec2(17.0, 16.0), Sense::click());
            let dn_resp = dn_resp.on_hover_text("Octave down (-1)");
            if dn_resp.clicked() && step.octave > -2 {
                step.octave -= 1;
                changed = true;
            }
            if ui.is_rect_visible(dn_rect) {
                let p = ui.painter();
                p.rect_filled(
                    dn_rect,
                    2.0,
                    if dn_resp.hovered() {
                        Color32::from_rgb(0x32, 0x38, 0x44)
                    } else {
                        Color32::from_rgb(0x20, 0x24, 0x2c)
                    },
                );
                p.rect_stroke(
                    dn_rect,
                    2.0,
                    Stroke::new(1.0_f32, theme::PANEL_BORDER),
                    nih_plug_egui::egui::StrokeKind::Inside,
                );
                let cx = dn_rect.center().x;
                let cy = dn_rect.center().y;
                let arrow_color = if step.octave > -2 {
                    theme::TEXT_PRIMARY
                } else {
                    theme::TEXT_MUTED
                };
                p.line_segment(
                    [pos2(cx - 2.5, cy - 1.5), pos2(cx, cy + 2.0)],
                    Stroke::new(1.3_f32, arrow_color),
                );
                p.line_segment(
                    [pos2(cx, cy + 2.0), pos2(cx + 2.5, cy - 1.5)],
                    Stroke::new(1.3_f32, arrow_color),
                );
            }
        });
    });

    changed
}
