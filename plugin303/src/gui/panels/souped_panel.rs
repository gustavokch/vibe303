use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    vec2, Button, CornerRadius, FontId, Frame, Margin, Separator, Stroke, Ui,
};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::{CircuitTopology, Plugin303Params};

pub fn render_souped_panel(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let is_accurate = params.topology.value() == CircuitTopology::Accurate;

    // Fixed content height for both FUN and ACCURATE states so regions below
    // do not reflow when the topology changes.
    const SOUPED_CONTENT_H: f32 = 104.0;

    Frame::new()
        .fill(theme::PANEL_BG_SOUPED)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(SOUPED_CONTENT_H);

            if is_accurate {
                // Centered overlay card when bypassed with zero ghost rendering
                let total_w = ui.available_width();
                // Guard against narrow drawer: clamp the available width slice so
                // card_w never underflows to 0 (which would collapse the Frame).
                const MIN_CARD_W: f32 = 120.0;
                let card_w = 300.0_f32.min((total_w - 20.0).max(MIN_CARD_W));
                let card_h = (SOUPED_CONTENT_H - 20.0).max(24.0);
                let pad = ((SOUPED_CONTENT_H - card_h) * 0.5).max(0.0);

                ui.vertical(|ui| {
                    ui.add_space(pad);
                    ui.vertical_centered(|ui| {
                        Frame::new()
                            .fill(theme::SURFACE_ELEVATED)
                            .stroke(Stroke::new(1.0_f32, theme::BORDER_FOCUS))
                            .corner_radius(CornerRadius::same(3))
                            .inner_margin(Margin::symmetric(16, 6))
                            .show(ui, |ui| {
                                ui.set_width(card_w);
                                ui.set_min_height(card_h);
                                ui.vertical_centered(|ui| {
                                    ui.label(
                                        nih_plug_egui::egui::RichText::new("ACCURATE MODE ACTIVE")
                                            .font(FontId::new(10.5, nih_plug_egui::egui::FontFamily::Proportional))
                                            .strong()
                                            .color(theme::TEXT_PRIMARY),
                                    );
                                    ui.add_space(2.0);
                                    ui.label(
                                        nih_plug_egui::egui::RichText::new("Souped-up engine is bypassed")
                                            .font(FontId::new(8.5, nih_plug_egui::egui::FontFamily::Proportional))
                                            .color(theme::TEXT_SECONDARY),
                                    );
                                    ui.add_space(6.0);

                                    let btn = Button::new(
                                        nih_plug_egui::egui::RichText::new("ENABLE FUN MODE")
                                            .font(FontId::new(9.0, nih_plug_egui::egui::FontFamily::Proportional))
                                            .strong()
                                            .color(theme::TEXT_ON_ACCENT),
                                    )
                                    .fill(theme::ACCENT_ORANGE)
                                    .min_size(vec2(130.0, 20.0))
                                    .corner_radius(CornerRadius::same(2));

                                    if ui.add(btn).clicked() {
                                        setter.begin_set_parameter(&params.topology);
                                        setter.set_parameter(&params.topology, CircuitTopology::Fun);
                                        setter.end_set_parameter(&params.topology);

                                        setter.begin_set_parameter(&params.reso_comp);
                                        setter.set_parameter_normalized(&params.reso_comp, 0.5);
                                        setter.end_set_parameter(&params.reso_comp);
                                    }
                                });
                            });
                    });
                });
            } else {
                ui.vertical(|ui| {
                    // Header Bar
                    ui.horizontal(|ui| {
                        ui.label(
                            nih_plug_egui::egui::RichText::new("SOUPED-UP ENGINE & ANALOG DRAWER")
                                .font(FontId::new(10.0, nih_plug_egui::egui::FontFamily::Proportional))
                                .strong()
                                .color(theme::ACCENT_ORANGE),
                        );
                    });

                    ui.add_space(3.0);

                    // Row 1: Selectors & Mode Toggles
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(10.0, 0.0);

                        ui.add(SlideSwitch::new(
                            &params.vcf_taps,
                            setter,
                            "VCF TAPS",
                            &["2-POLE", "3-POLE", "4-POLE"],
                        ));

                        ui.add_space(8.0);

                        ui.add(SlideSwitch::new(
                            &params.vca_env_mode,
                            setter,
                            "VCA ENV",
                            &["GATE", "ENV"],
                        ));
                    });

                    ui.add_space(2.0);
                    ui.separator();
                    ui.add_space(2.0);

                    // Row 2: Engine Rotary Dials (9 knobs across full width,
                    // split into three visually separated groups).
                    const KNOB_GROUP_SEP_W: f32 = 2.0;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                        let total_w = ui.available_width();
                        let col_w = (total_w - 2.0 * KNOB_GROUP_SEP_W) / 9.0;

                        let render_one = |ui: &mut Ui, knob: Knob303<'_, _>| {
                            ui.allocate_ui_with_layout(
                                vec2(col_w, 56.0),
                                nih_plug_egui::egui::Layout::top_down(nih_plug_egui::egui::Align::Center),
                                |ui| {
                                    ui.add(knob);
                                },
                            );
                        };

                        render_one(ui, Knob303::new(&params.vcf_drive, setter, "DRIVE", 12.0));
                        render_one(ui, Knob303::new(&params.sub_vol, setter, "SUB", 12.0));
                        ui.add(Separator::default().vertical());
                        render_one(ui, Knob303::new(&params.glide, setter, "GLIDE", 12.0));
                        render_one(ui, Knob303::new(&params.osc1_pitch, setter, "PITCH1", 12.0));
                        render_one(ui, Knob303::new(&params.osc2_pitch, setter, "PITCH2", 12.0));
                        render_one(ui, Knob303::new(&params.osc2_vol, setter, "MIX2", 12.0));
                        ui.add(Separator::default().vertical());
                        render_one(ui, Knob303::new(&params.osc1_pw, setter, "PW", 12.0));
                        render_one(ui, Knob303::new(&params.sweep_pos, setter, "SWEEP", 12.0));
                        render_one(ui, Knob303::new(&params.reso_comp, setter, "R-COMP", 12.0));
                    });
                });
            }
        });
}
