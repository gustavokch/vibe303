use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    pos2, vec2, Align, Align2, ComboBox, FontFamily, FontId, Layout, Rect, RichText, Sense, Ui,
    UiBuilder,
};

use crate::gui::theme;
use crate::gui::widgets::SlideSwitch;
use crate::gui::HEADER_BAR_H;
use crate::params::{CircuitTopology, Plugin303Params};

const ZOOM_PRESETS: [(f32, &str); 2] = [(0.75, "75%"), (1.0, "100%")];

/// Horizontal inset of the bar's contents from its edge.
const PAD_X: f32 = 8.0;

fn caption(text: &str) -> RichText {
    RichText::new(text)
        .font(FontId::new(8.5, FontFamily::Proportional))
        .color(theme::TEXT_SECONDARY)
}

/// The app bar: instrument identity on the left, engine-wide modes on the
/// right. One flat strip, one row, every control on the same 18 px height
/// (`SEG_H`, `TOGGLE_H`).
pub fn render_header(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, HEADER_BAR_H), Sense::hover());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        p.rect_filled(rect, theme::radius(), theme::PANEL_HEADER_BG);
        // Title block. The TECHNEO 303 name is not repeated here: the window
        // title already carries it.
        let title = p.text(
            pos2(rect.left() + PAD_X, rect.center().y),
            Align2::LEFT_CENTER,
            "BASS SYNTH",
            FontId::new(9.5, FontFamily::Proportional),
            theme::TEXT_PRIMARY,
        );
        p.text(
            pos2(title.right() + 8.0, rect.center().y),
            Align2::LEFT_CENTER,
            "MONO",
            FontId::new(9.5, FontFamily::Proportional),
            theme::TEXT_SECONDARY,
        );
    }

    let mut controls = ui.new_child(
        UiBuilder::new()
            .id_salt("header_controls")
            .max_rect(Rect::from_min_max(
                pos2(rect.left() + PAD_X, rect.top()),
                pos2(rect.right() - PAD_X, rect.bottom()),
            ))
            .layout(Layout::right_to_left(Align::Center)),
    );
    let ui = &mut controls;
    ui.spacing_mut().item_spacing.x = 16.0;

    // Zoom factor dropdown (rightmost).
    let current_zoom = ui.ctx().zoom_factor();
    let zoom_label = ZOOM_PRESETS
        .iter()
        .find(|(z, _)| (current_zoom - z).abs() < 0.01)
        .map(|(_, l)| *l)
        .unwrap_or("100%");
    ui.scope(|ui| {
        theme::field_style(ui);
        ui.spacing_mut().item_spacing.x = 6.0;
        ComboBox::from_id_salt("header_zoom_selector")
            .selected_text(
                RichText::new(zoom_label).font(FontId::new(8.5, FontFamily::Proportional)),
            )
            .width(54.0)
            .show_ui(ui, |ui| {
                for &(preset, label) in &ZOOM_PRESETS {
                    let is_selected = (current_zoom - preset).abs() < 0.01;
                    if ui
                        .selectable_label(
                            is_selected,
                            RichText::new(label).font(FontId::new(8.5, FontFamily::Proportional)),
                        )
                        .clicked()
                    {
                        ui.ctx().set_zoom_factor(preset);
                    }
                }
            });
        // Right-to-left: added after the combo, so it lands on its left.
        ui.label(caption("ZOOM"));
    });

    // Quality tier (inert in ACCURATE: that mode uses fixed physical modelling).
    let is_accurate = params.topology.value() == CircuitTopology::Accurate;
    let quality_resp = ui.add_enabled_ui(!is_accurate, |ui| {
        ui.add(
            SlideSwitch::new(
                &params.tier,
                setter,
                "QUALITY",
                &["LOW", "HIGH", "ULTRA", "ULTRA 2X"],
            )
            .caption_left(),
        )
    });
    if is_accurate {
        quality_resp.response.on_hover_text(
            "Quality tier applies to FUN mode (ACCURATE uses fixed physical modeling)",
        );
    }

    // Circuit topology.
    let prev_topology = params.topology.value();
    let switch_resp = ui.add(
        SlideSwitch::new(&params.topology, setter, "TOPOLOGY", &["ACCURATE", "FUN"]).caption_left(),
    );
    if switch_resp.changed() {
        let new_topology = params.topology.value();
        if new_topology != prev_topology {
            let target_comp = match new_topology {
                CircuitTopology::Accurate => 0.0,
                CircuitTopology::Fun => 0.5,
            };
            setter.begin_set_parameter(&params.reso_comp);
            setter.set_parameter_normalized(&params.reso_comp, target_comp);
            setter.end_set_parameter(&params.reso_comp);
        }
    }
}
