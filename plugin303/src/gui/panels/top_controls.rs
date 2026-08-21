use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{pos2, vec2, Color32, CornerRadius, FontId, Frame, Margin, Stroke, Ui};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, MiniFader};
use crate::params::Plugin303Params;

pub fn render_top_controls(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let desired_size = vec2(ui.available_width(), 100.0);
    let (rect, _) = ui.allocate_exact_size(desired_size, nih_plug_egui::egui::Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();

        painter.rect_filled(rect, 4.0, theme::PANEL_BG);

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
        nih_plug_egui::egui::UiBuilder::new().max_rect(rect.shrink(8.0)),
        |ui| {
            ui.horizontal(|ui| {
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
                ui.add_space(8.0);

                // ADV ENV checkbox
                let mut adv_env = params.advanced_env.value();
                if ui
                    .checkbox(
                        &mut adv_env,
                        nih_plug_egui::egui::RichText::new("ADV\nENV")
                            .font(FontId::proportional(8.5))
                            .color(theme::TEXT_SECONDARY),
                    )
                    .changed()
                {
                    setter.begin_set_parameter(&params.advanced_env);
                    setter.set_parameter(&params.advanced_env, adv_env);
                    setter.end_set_parameter(&params.advanced_env);
                }

                ui.add_space(4.0);

                // MEG ADSR & VEG ADSR Mini-Fader Banks
                let bank_frame = Frame::new()
                    .fill(Color32::from_rgb(0x1b, 0x1b, 0x1b))
                    .stroke(Stroke::new(
                        1.0_f32,
                        Color32::from_rgb(0x2a, 0x2e, 0x33),
                    ))
                    .corner_radius(CornerRadius::same(3))
                    .inner_margin(Margin::symmetric(4, 3));

                // MEG ADSR Group
                bank_frame.show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            nih_plug_egui::egui::RichText::new("MEG ADSR")
                                .font(FontId::new(
                                    8.5,
                                    nih_plug_egui::egui::FontFamily::Proportional,
                                ))
                                .strong()
                                .color(theme::TEXT_PRIMARY),
                        );
                        ui.add_space(1.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                            ui.add(MiniFader::new(
                                &params.meg_attack,
                                setter,
                                "A",
                                30.0,
                                theme::SOUPED_ORANGE,
                            ));
                            ui.add(MiniFader::new(
                                &params.meg_decay_adj,
                                setter,
                                "D",
                                30.0,
                                theme::SOUPED_ORANGE,
                            ));
                            ui.add(MiniFader::new(
                                &params.meg_sustain,
                                setter,
                                "S",
                                30.0,
                                theme::SOUPED_ORANGE,
                            ));
                            ui.add(MiniFader::new(
                                &params.meg_release,
                                setter,
                                "R",
                                30.0,
                                theme::SOUPED_ORANGE,
                            ));
                        });
                    });
                });

                ui.add_space(4.0);

                // VEG ADSR Group
                bank_frame.show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            nih_plug_egui::egui::RichText::new("VEG ADSR")
                                .font(FontId::new(
                                    8.5,
                                    nih_plug_egui::egui::FontFamily::Proportional,
                                ))
                                .strong()
                                .color(theme::TEXT_PRIMARY),
                        );
                        ui.add_space(1.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                            ui.add(MiniFader::new(
                                &params.veg_attack,
                                setter,
                                "A",
                                30.0,
                                theme::SOUPED_ORANGE,
                            ));
                            ui.add(MiniFader::new(
                                &params.veg_decay_adj,
                                setter,
                                "D",
                                30.0,
                                theme::SOUPED_ORANGE,
                            ));
                            ui.add(MiniFader::new(
                                &params.veg_sustain,
                                setter,
                                "S",
                                30.0,
                                theme::SOUPED_ORANGE,
                            ));
                            ui.add(MiniFader::new(
                                &params.veg_release,
                                setter,
                                "R",
                                30.0,
                                theme::SOUPED_ORANGE,
                            ));
                        });
                    });
                });
            });
        },
    );

    // Procedural corner mounting screws
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
        painter.line_segment(
            [pos2(pos.x - dx, pos.y - dy), pos2(pos.x + dx, pos.y + dy)],
            slot_stroke,
        );
        painter.line_segment(
            [pos2(pos.x + dy, pos.y - dx), pos2(pos.x - dy, pos.y + dx)],
            slot_stroke,
        );
    }
}
