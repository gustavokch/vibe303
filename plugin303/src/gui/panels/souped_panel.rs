use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    vec2, Button, CornerRadius, FontId, Frame, Margin, Stroke, Ui,
};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::{CircuitTopology, Plugin303Params};

pub fn render_souped_panel(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let is_accurate = params.topology.value() == CircuitTopology::Accurate;

    // Content height matching the FUN knob row so regions below do not
    // reflow when the topology changes (FUN frame outer = 131 + 17 = 148).
    const SOUPED_CONTENT_H: f32 = 131.0;

    Frame::new()
        .fill(theme::PANEL_BG_SOUPED)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(SOUPED_CONTENT_H);

            if is_accurate {
                // Overlay card fills the whole panel region (fix #5) and is
                // hard-clipped to it (fix #2): the card can never paint over
                // the tweaks panel below, even mid-switch.
                let total_w = ui.available_width();
                let (region_rect, _) = ui.allocate_exact_size(
                    vec2(total_w, SOUPED_CONTENT_H),
                    nih_plug_egui::egui::Sense::hover(),
                );
                let clip = ui.clip_rect().intersect(region_rect);
                // The card lives inside an 8px-inset rect: a Frame's outer
                // rect is content + inner_margin + stroke, so forcing the
                // card content to the FULL region width inflates the card
                // (and, via allocate_new_ui, every panel below it) past the
                // region edges (round-4 left-shift regression).
                let card_rect = region_rect.shrink(8.0);
                ui.allocate_new_ui(
                    nih_plug_egui::egui::UiBuilder::new()
                        .max_rect(card_rect)
                        .layout(nih_plug_egui::egui::Layout::top_down(
                            nih_plug_egui::egui::Align::Center,
                        )),
                    |ui| {
                        ui.set_clip_rect(clip);
                        Frame::new()
                    .fill(theme::SURFACE_ELEVATED)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER_FOCUS))
                    .corner_radius(CornerRadius::same(3))
                    .inner_margin(Margin::symmetric(16, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.set_min_height(ui.available_height());
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
                    },
                );
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

                    // Row 2: Engine Rotary Dials (9 knobs across full width,
                    // split into three visually separated groups).
                    // Group rules are painted 2px columns, NOT egui Separators:
                    // Separator::default() allocates ~6px each and overflowed
                    // the panel width by 8px (round-3 regression).
                    const KNOB_GROUP_SEP_W: f32 = 2.0;
                    let paint_group_rule = |ui: &mut Ui| {
                        let (rule_rect, _) = ui.allocate_exact_size(
                            vec2(KNOB_GROUP_SEP_W, 56.0),
                            nih_plug_egui::egui::Sense::hover(),
                        );
                        if ui.is_rect_visible(rule_rect) {
                            ui.painter().vline(
                                rule_rect.center().x,
                                rule_rect.y_range(),
                                Stroke::new(1.0_f32, theme::BORDER_SUBTLE),
                            );
                        }
                    };
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
                        paint_group_rule(ui);
                        render_one(ui, Knob303::new(&params.glide, setter, "GLIDE", 12.0));
                        render_one(ui, Knob303::new(&params.osc1_pitch, setter, "PITCH1", 12.0));
                        render_one(ui, Knob303::new(&params.osc2_pitch, setter, "PITCH2", 12.0));
                        render_one(ui, Knob303::new(&params.osc2_vol, setter, "MIX2", 12.0));
                        paint_group_rule(ui);
                        render_one(ui, Knob303::new(&params.osc1_pw, setter, "PW", 12.0));
                        render_one(ui, Knob303::new(&params.sweep_pos, setter, "SWEEP", 12.0));
                        render_one(ui, Knob303::new(&params.reso_comp, setter, "R-COMP", 12.0));
                    });
                });
            }
        });
}
