use nih_plug::prelude::{FloatParam, ParamSetter};
use nih_plug_egui::egui::{pos2, vec2, Color32, CornerRadius, FontId, Frame, Margin, Stroke, Ui};

use crate::gui::theme;
use crate::gui::widgets::{Knob303, MiniFader};
use crate::params::Plugin303Params;

fn render_adsr_bank(
    ui: &mut Ui,
    title: &str,
    attack: &FloatParam,
    decay: &FloatParam,
    sustain: &FloatParam,
    release: &FloatParam,
    setter: &ParamSetter,
) {
    let bank_frame = Frame::new()
        .fill(theme::BANK_FRAME_BG)
        .stroke(Stroke::new(1.0_f32, theme::BANK_FRAME_BORDER))
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::symmetric(4, 3));

    bank_frame.show(ui, |ui| {
        ui.vertical(|ui| {
            ui.label(
                nih_plug_egui::egui::RichText::new(title)
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
                    attack,
                    setter,
                    "A",
                    30.0,
                    theme::SOUPED_ORANGE,
                ));
                ui.add(MiniFader::new(
                    decay,
                    setter,
                    "D",
                    30.0,
                    theme::SOUPED_ORANGE,
                ));
                ui.add(MiniFader::new(
                    sustain,
                    setter,
                    "S",
                    30.0,
                    theme::SOUPED_ORANGE,
                ));
                ui.add(MiniFader::new(
                    release,
                    setter,
                    "R",
                    30.0,
                    theme::SOUPED_ORANGE,
                ));
            });
        });
    });
}

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
            Stroke::new(1.0_f32, theme::CHASSIS_BG),
            nih_plug_egui::egui::StrokeKind::Inside,
        );
    }

    ui.allocate_new_ui(
        nih_plug_egui::egui::UiBuilder::new().max_rect(rect.shrink(8.0)),
        |ui| {
            ui.horizontal(|ui| {
                ui.add_space(6.0);

                // Classic 303 Parameter Gauges (7 Knobs)
                ui.spacing_mut().item_spacing = vec2(16.0, 0.0);
                ui.add(Knob303::new(&params.tuning, setter, "TUNING", 18.0));
                ui.add(Knob303::new(&params.cutoff, setter, "CUTOFF", 18.0));
                ui.add(Knob303::new(&params.resonance, setter, "RESONANCE", 18.0));
                ui.add(Knob303::new(&params.env_mod, setter, "ENV MOD", 18.0));
                ui.add(Knob303::new(&params.decay, setter, "DECAY", 18.0));
                ui.add(Knob303::new(&params.accent, setter, "ACCENT", 18.0));
                ui.add(Knob303::new(&params.volume, setter, "VOLUME", 18.0));

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(14.0);

                // Envelopes Module Frame: contains ADV ENV toggle in header + MEG ADSR + VEG ADSR
                let env_frame = Frame::new()
                    .fill(theme::BANK_FRAME_BG)
                    .stroke(Stroke::new(1.0_f32, theme::BANK_FRAME_BORDER))
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(Margin::symmetric(6, 4));

                env_frame.show(ui, |ui| {
                    ui.vertical(|ui| {
                        // Header with Title and ADV ENV checkbox anchored together
                        ui.horizontal(|ui| {
                            ui.label(
                                nih_plug_egui::egui::RichText::new("ENVELOPES")
                                    .font(FontId::new(9.0, nih_plug_egui::egui::FontFamily::Proportional))
                                    .strong()
                                    .color(theme::TEXT_PRIMARY),
                            );
                            ui.add_space(8.0);
                            let mut adv_env = params.advanced_env.value();
                            if ui
                                .checkbox(
                                    &mut adv_env,
                                    nih_plug_egui::egui::RichText::new("ADV ENV")
                                        .font(FontId::proportional(8.5))
                                        .color(theme::TEXT_SECONDARY),
                                )
                                .changed()
                            {
                                setter.begin_set_parameter(&params.advanced_env);
                                setter.set_parameter(&params.advanced_env, adv_env);
                                setter.end_set_parameter(&params.advanced_env);
                            }
                        });

                        ui.add_space(2.0);

                        // ADSR banks row
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                            render_adsr_bank(
                                ui,
                                "MEG ADSR",
                                &params.meg_attack,
                                &params.meg_decay_adj,
                                &params.meg_sustain,
                                &params.meg_release,
                                setter,
                            );

                            render_adsr_bank(
                                ui,
                                "VEG ADSR",
                                &params.veg_attack,
                                &params.veg_decay_adj,
                                &params.veg_sustain,
                                &params.veg_release,
                                setter,
                            );
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
        painter.circle_filled(pos, 3.0, theme::CHASSIS_BG);
        painter.circle_filled(pos, 2.2, Color32::from_rgb(0x48, 0x48, 0x48));
        let angle = idx as f32 * 0.45;
        let dx = angle.cos() * 1.4;
        let dy = angle.sin() * 1.4;
        let slot_stroke = Stroke::new(0.9_f32, theme::CHASSIS_BG);
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
