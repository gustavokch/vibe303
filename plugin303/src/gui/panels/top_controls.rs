use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{pos2, vec2, Color32, Stroke, Ui};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::Plugin303Params;

pub fn render_top_controls(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    // Exact sizing for gradient background
    let desired_size = vec2(500.0, 190.0);
    let (rect, _) = ui.allocate_exact_size(desired_size, nih_plug_egui::egui::Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();

        // Metallic gradient background
        let mut mesh = nih_plug_egui::egui::Mesh::default();
        mesh.vertices.push(nih_plug_egui::egui::epaint::Vertex { pos: rect.left_top(), uv: pos2(0.0, 0.0), color: Color32::from_rgb(0x2a, 0x35, 0x48) });
        mesh.vertices.push(nih_plug_egui::egui::epaint::Vertex { pos: rect.right_top(), uv: pos2(0.0, 0.0), color: Color32::from_rgb(0x2a, 0x35, 0x48) });
        mesh.vertices.push(nih_plug_egui::egui::epaint::Vertex { pos: rect.left_bottom(), uv: pos2(0.0, 0.0), color: Color32::from_rgb(0x10, 0x16, 0x24) });
        mesh.vertices.push(nih_plug_egui::egui::epaint::Vertex { pos: rect.right_bottom(), uv: pos2(0.0, 0.0), color: Color32::from_rgb(0x10, 0x16, 0x24) });
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(1, 3, 2);
        painter.add(nih_plug_egui::egui::Shape::mesh(mesh));

        // Bevel stroke lines
        painter.rect_stroke(rect, 4.0, Stroke::new(1.5_f32, theme::PANEL_BORDER_HIGHLIGHT), nih_plug_egui::egui::StrokeKind::Inside);
        painter.rect_stroke(rect.shrink(1.5), 2.5, Stroke::new(1.0_f32, Color32::from_rgb(0x05, 0x08, 0x10)), nih_plug_egui::egui::StrokeKind::Inside);
    }

    ui.allocate_ui_at_rect(rect.shrink(10.0), |ui| {
        ui.vertical(|ui| {
            ui.set_width(480.0);
            // ROW 1: Classic 303 Parameter Gauges
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
            ui.add(Knob303::new(&params.tuning, setter, "TUNING", 20.0));
            ui.add(Knob303::new(&params.cutoff, setter, "CUTOFF", 20.0));
            ui.add(Knob303::new(&params.resonance, setter, "RESONANCE", 20.0));
            ui.add(Knob303::new(&params.env_mod, setter, "ENV MOD", 20.0));
            ui.add(Knob303::new(&params.decay, setter, "DECAY", 20.0));
            ui.add(Knob303::new(&params.accent, setter, "ACCENT", 20.0));
            ui.add(Knob303::new(&params.volume, setter, "VOLUME", 20.0));
        });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);

        // ROW 2: Waveform & Fidelity Tier Vertical Slide Switches
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.spacing_mut().item_spacing = vec2(30.0, 0.0);

            ui.add(
                SlideSwitch::new(
                    &params.waveform,
                    setter,
                    "WAVEFORM",
                    &["SAW", "SQUARE", "SWEEP"],
                )
                .vertical(),
            );

            ui.add(
                SlideSwitch::new(
                    &params.tier,
                    setter,
                    "FIDELITY TIER",
                    &["LOW", "HIGH", "ULTRA"],
                )
                .with_thumb_color(Color32::from_rgb(0x8b, 0x00, 0x00))
                
            );
        });
        });
    });

    // Procedural corner mounting screws with realistic cross-head slots
    let painter = ui.painter();
    let screw_positions = [
        pos2(rect.left() + 7.0, rect.top() + 7.0),
        pos2(rect.right() - 7.0, rect.top() + 7.0),
        pos2(rect.left() + 7.0, rect.bottom() - 7.0),
        pos2(rect.right() - 7.0, rect.bottom() - 7.0),
    ];
    for (idx, &pos) in screw_positions.iter().enumerate() {
        painter.circle_filled(pos, 3.0, Color32::from_rgb(0x10, 0x16, 0x24));
        painter.circle_filled(pos, 2.2, Color32::from_rgb(0x47, 0x55, 0x69));
        let angle = idx as f32 * 0.45;
        let dx = angle.cos() * 1.4;
        let dy = angle.sin() * 1.4;
        let slot_stroke = Stroke::new(0.9_f32, Color32::from_rgb(0x0f, 0x17, 0x2a));
        painter.line_segment([pos2(pos.x - dx, pos.y - dy), pos2(pos.x + dx, pos.y + dy)], slot_stroke);
        painter.line_segment([pos2(pos.x + dy, pos.y - dx), pos2(pos.x - dy, pos.y + dx)], slot_stroke);
    }
}
