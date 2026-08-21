use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::epaint::Mesh;
use nih_plug_egui::egui::{pos2, vec2, Align2, Color32, FontFamily, FontId, Rect, Sense, Stroke, Ui};

use crate::gui::theme;
use crate::gui::widgets::SlideSwitch;
use crate::params::Plugin303Params;

const LOGO_LIGHT: Color32 = Color32::from_rgb(0xe8, 0xec, 0xf0);
const LOGO_DARK: Color32 = Color32::from_rgb(0x84, 0x8c, 0x96);
const LOGO_ACCENT: Color32 = Color32::from_rgb(0xff, 0x88, 0x00);
const LOGO_ACCENT_LIGHT: Color32 = Color32::from_rgb(0xff, 0xa8, 0x40);

const BAND_TOP: Color32 = Color32::from_rgb(0x23, 0x26, 0x2b);
const BAND_BOTTOM: Color32 = Color32::from_rgb(0x16, 0x18, 0x1b);
const BAND_BEVEL: Color32 = Color32::from_rgb(0x2a, 0x2e, 0x33);

/// Vertical gradient rect (two triangles) for the brushed-steel header band.
fn gradient_rect(painter: &nih_plug_egui::egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(nih_plug_egui::egui::Shape::mesh(mesh));
}

/// Two-tone metallic logotype: dark base pass, light pass clipped to the top
/// half. Reads as a brushed silver gradient with an orange accent glyph.
fn paint_metallic_text(
    ui: &Ui,
    origin: nih_plug_egui::egui::Pos2,
    font: &FontId,
    main: &str,
    accent: &str,
) -> f32 {
    let painter = ui.painter();
    let main_w = ui.fonts(|f| f.layout_no_wrap(main.to_string(), font.clone(), Color32::WHITE).size().x);
    let accent_w = if accent.is_empty() {
        0.0
    } else {
        ui.fonts(|f| f.layout_no_wrap(accent.to_string(), font.clone(), Color32::WHITE).size().x)
    };
    let text_h = ui.fonts(|f| f.row_height(font));

    let accent_x = origin.x + main_w;

    // Dark base pass (full text)
    painter.text(origin, Align2::LEFT_CENTER, main, font.clone(), LOGO_DARK);
    if !accent.is_empty() {
        painter.text(
            pos2(accent_x, origin.y),
            Align2::LEFT_CENTER,
            accent,
            font.clone(),
            LOGO_ACCENT,
        );
    }

    // Light pass clipped to the upper 55% of the cap height
    let clip_top = origin.y - text_h * 0.55;
    let clip_bottom = origin.y - text_h * 0.05;
    let clip = Rect::from_min_max(
        pos2(origin.x - 2.0, clip_top),
        pos2(accent_x + accent_w + 2.0, clip_bottom),
    );
    let light_painter = painter.with_clip_rect(clip);
    light_painter.text(origin, Align2::LEFT_CENTER, main, font.clone(), LOGO_LIGHT);
    if !accent.is_empty() {
        light_painter.text(
            pos2(accent_x, origin.y),
            Align2::LEFT_CENTER,
            accent,
            font.clone(),
            LOGO_ACCENT_LIGHT,
        );
    }

    main_w + accent_w
}

/// One bank of recessed vent slits: near-black groove with a 1px bottom highlight.
fn paint_vent_bank(painter: &nih_plug_egui::egui::Painter, center_x: f32, center_y: f32, slits: usize) {
    let slit_h = 18.0;
    let pitch = 5.0;
    let bank_w = (slits as f32 - 1.0) * pitch;
    let x0 = center_x - bank_w / 2.0;
    for i in 0..slits {
        let x = x0 + i as f32 * pitch;
        let top = pos2(x, center_y - slit_h / 2.0);
        let bottom = pos2(x, center_y + slit_h / 2.0);
        painter.line_segment([top, bottom], Stroke::new(2.0_f32, Color32::from_rgb(0x0a, 0x0b, 0x0d)));
        painter.line_segment(
            [top + vec2(0.0, 1.0), bottom + vec2(0.0, 1.0)],
            Stroke::new(0.75_f32, Color32::from_rgb(0x3a, 0x3a, 0x3a)),
        );
    }
}

pub fn render_header(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let content_rect = ui.max_rect();
    let strip_cx = content_rect.center().x;
    let strip_top = ui.cursor().min.y;
    let strip_cy = strip_top + 11.0;

    // Brushed-steel band: vertical gradient + bottom bevel line
    let band_rect = Rect::from_min_max(
        pos2(content_rect.left(), strip_top),
        pos2(content_rect.right(), strip_top + 22.0),
    );
    let painter = ui.painter();
    gradient_rect(painter, band_rect, BAND_TOP, BAND_BOTTOM);
    painter.line_segment(
        [band_rect.left_bottom(), band_rect.right_bottom()],
        Stroke::new(1.0_f32, BAND_BEVEL),
    );
    // Faint warm under-glow along the header's lower edge (kept dim: a warm
    // reflection, not an accent rule)
    painter.line_segment(
        [
            band_rect.left_bottom() - vec2(0.0, 1.0),
            band_rect.right_bottom() - vec2(0.0, 1.0),
        ],
        Stroke::new(
            1.0_f32,
            Color32::from_rgba_unmultiplied(0xff, 0x88, 0x00, 0x10),
        ),
    );

    ui.horizontal(|ui| {
        ui.set_height(22.0);

        // Metallic TECHNEO logotype (TECHN silver, EO orange)
        ui.add_space(6.0);
        let logo_font = FontId::new(16.0, FontFamily::Proportional);
        let logo_origin = pos2(ui.cursor().min.x, strip_cy);
        let logo_w = paint_metallic_text(ui, logo_origin, &logo_font, "TECH", "NEO");
        ui.allocate_exact_size(vec2(logo_w, 20.0), Sense::hover());

        // Vent slit banks flanking the center of the header, widely spaced
        let painter = ui.painter();
        paint_vent_bank(painter, strip_cx - 150.0, strip_cy, 12);
        paint_vent_bank(painter, strip_cx + 150.0, strip_cy, 12);

        ui.with_layout(
            nih_plug_egui::egui::Layout::right_to_left(nih_plug_egui::egui::Align::Center),
            |ui| {
                ui.add_space(4.0);
                ui.label(
                    nih_plug_egui::egui::RichText::new("TB-303 PLUGIN")
                        .font(FontId::new(11.0, FontFamily::Proportional))
                        .strong()
                        .color(theme::SOUPED_ORANGE),
                );
                ui.add_space(12.0);
                ui.add(SlideSwitch::new(
                    &params.tier,
                    setter,
                    "QUALITY",
                    &["LOW", "HIGH", "ULTRA"],
                ));
                ui.add_space(8.0);
                let prev_topology = params.topology.value();
                let switch_resp = ui.add(SlideSwitch::new(
                    &params.topology,
                    setter,
                    "CIRCUIT TOPOLOGY",
                    &["ACCURATE", "FUN"],
                ));
                if switch_resp.changed() {
                    let new_topology = params.topology.value();
                    if new_topology != prev_topology {
                        let target_comp = match new_topology {
                            crate::params::CircuitTopology::Accurate => 0.0,
                            crate::params::CircuitTopology::Fun => 0.5,
                        };
                        setter.begin_set_parameter(&params.reso_comp);
                        setter.set_parameter_normalized(&params.reso_comp, target_comp);
                        setter.end_set_parameter(&params.reso_comp);
                    }
                }
            },
        );
    });
}
