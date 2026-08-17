use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{pos2, vec2, Color32, Stroke, Ui};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::Plugin303Params;

pub fn render_top_controls(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    // Exact sizing for panel background
    let desired_size = vec2(1060.0, 110.0);
    let (rect, _) = ui.allocate_exact_size(desired_size, nih_plug_egui::egui::Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();

        // Flat dark grey background panel (#242424)
        painter.rect_filled(rect, 4.0, theme::PANEL_BG);

        // Bevel stroke lines
        painter.rect_stroke(
            rect,
            4.0,
            theme::border_stroke(),
            nih_plug_egui::egui::StrokeKind::Inside,
        );
        painter.rect_stroke(
            rect.shrink(1.5),
            2.5,
            Stroke::new(1.0_f32, Color32::from_rgb(0x18, 0x18, 0x18)),
            nih_plug_egui::egui::StrokeKind::Inside,
        );
    }

    ui.allocate_new_ui(
        nih_plug_egui::egui::UiBuilder::new().max_rect(rect.shrink(10.0)),
        |ui| {
            ui.horizontal(|ui| {
                ui.set_width(1040.0);
                // Classic 303 Parameter Gauges (7 Knobs)
                ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                ui.add(Knob303::new(&params.tuning, setter, "TUNING", 18.0));
                ui.add(Knob303::new(&params.cutoff, setter, "CUTOFF", 18.0));
                ui.add(Knob303::new(&params.resonance, setter, "RESONANCE", 18.0));
                ui.add(Knob303::new(&params.env_mod, setter, "ENV MOD", 18.0));
                ui.add(Knob303::new(&params.decay, setter, "DECAY", 18.0));
                ui.add(Knob303::new(&params.accent, setter, "ACCENT", 18.0));
                ui.add(Knob303::new(&params.volume, setter, "VOLUME", 18.0));

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(12.0);

                // Waveform & Fidelity Tier Slide Switches
                ui.spacing_mut().item_spacing = vec2(16.0, 0.0);
                ui.add(
                    SlideSwitch::new(
                        &params.waveform,
                        setter,
                        "WAVEFORM",
                        &["SAW", "SQUARE", "SWEEP"],
                    ),
                );

                ui.add(
                    SlideSwitch::new(
                        &params.tier,
                        setter,
                        "FIDELITY TIER",
                        &["LOW", "HIGH", "ULTRA"],
                    )
                    .with_thumb_color(Color32::from_rgb(0x8b, 0x00, 0x00)),
                );
            });
        },
    );

    // Procedural corner mounting screws with realistic cross-head slots
    let painter = ui.painter();
    let screw_positions = [
        pos2(rect.left() + 7.0, rect.top() + 7.0),
        pos2(rect.right() - 7.0, rect.top() + 7.0),
        pos2(rect.left() + 7.0, rect.bottom() - 7.0),
        pos2(rect.right() - 7.0, rect.bottom() - 7.0),
    ];
    for (idx, &pos) in screw_positions.iter().enumerate() {
        painter.circle_filled(pos, 3.0, Color32::from_rgb(0x18, 0x18, 0x18));
        painter.circle_filled(pos, 2.2, Color32::from_rgb(0x48, 0x48, 0x48));
        let angle = idx as f32 * 0.45;
        let dx = angle.cos() * 1.4;
        let dy = angle.sin() * 1.4;
        let slot_stroke = Stroke::new(0.9_f32, Color32::from_rgb(0x18, 0x18, 0x18));
        painter.line_segment([pos2(pos.x - dx, pos.y - dy), pos2(pos.x + dx, pos.y + dy)], slot_stroke);
        painter.line_segment([pos2(pos.x + dy, pos.y - dx), pos2(pos.x - dy, pos.y + dx)], slot_stroke);
    }
}
