use nih_plug::prelude::{Param, ParamSetter};
use nih_plug_egui::egui::{vec2, Align, Button, FontId, Layout, RichText, Ui};

use crate::gui::theme;
use crate::gui::widgets::panel::DevicePanel;
use crate::gui::widgets::{Knob303, SlideSwitch};
use crate::params::{CircuitTopology, Plugin303Params};

pub fn render_souped_panel(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let is_accurate = params.topology.value() == CircuitTopology::Accurate;

    DevicePanel::new("SOUPED-UP ENGINE", crate::gui::BANK_ROW_H)
        .dimmed(is_accurate)
        .show(
            ui,
            |header_ui| {
                if !is_accurate {
                    // Header runs right-to-left: add VCA ENV first, then VCF TAPS.
                    header_ui.add(
                        SlideSwitch::new(&params.vca_env_mode, setter, "VCA ENV", &["GATE", "ENV"])
                            .caption_left(),
                    )
                    .on_hover_text(
                        "GATE: the VCA is open while the gate is held. ENV: the VCA follows the \
                         amplitude envelope.",
                    );
                    header_ui
                        .add(
                            SlideSwitch::new(
                                &params.vcf_taps,
                                setter,
                                "VCF TAPS",
                                &["2-POLE", "3-POLE", "4-POLE"],
                            )
                            .caption_left(),
                        )
                        .on_hover_text("Filter slope: 2, 3 or 4 poles");
                }
            },
            |body_ui| {
                if is_accurate {
                    // Vertically and horizontally centered single-row banner in the 50 px body
                    let pad_y = ((body_ui.available_height()
                        - crate::gui::widgets::toggle::TOGGLE_H)
                        * 0.5)
                        .max(0.0);
                    body_ui.add_space(pad_y);

                    let font_title = FontId::proportional(10.5);
                    let font_sub = FontId::proportional(8.5);
                    let text_title = "ACCURATE MODE ACTIVE";
                    let text_sub = "Souped-up engine is bypassed";
                    let btn_text = "ENABLE FUN MODE";
                    let btn_w = 120.0;
                    let spacing = 12.0;

                    let (w_title, w_sub) = body_ui.fonts(|f| {
                        (
                            f.layout_no_wrap(
                                text_title.to_string(),
                                font_title.clone(),
                                theme::TEXT_PRIMARY,
                            )
                            .size()
                            .x,
                            f.layout_no_wrap(
                                text_sub.to_string(),
                                font_sub.clone(),
                                theme::TEXT_SECONDARY,
                            )
                            .size()
                            .x,
                        )
                    });
                    let total_row_w = w_title + spacing + w_sub + spacing + btn_w;
                    let pad_x = ((body_ui.available_width() - total_row_w) * 0.5).max(0.0);

                    body_ui.horizontal(|ui| {
                        ui.add_space(pad_x);
                        ui.spacing_mut().item_spacing.x = spacing;

                        ui.label(
                            RichText::new(text_title)
                                .font(font_title)
                                .color(theme::TEXT_PRIMARY),
                        );
                        ui.label(
                            RichText::new(text_sub)
                                .font(font_sub)
                                .color(theme::TEXT_SECONDARY),
                        );

                        let btn = Button::new(
                            RichText::new(btn_text)
                                .font(FontId::proportional(8.5))
                                .color(theme::TEXT_ON_ACCENT),
                        )
                        .fill(theme::ACCENT_ORANGE)
                        .min_size(vec2(btn_w, crate::gui::widgets::toggle::TOGGLE_H));

                        if ui.add(btn).clicked() {
                            setter.begin_set_parameter(&params.topology);
                            setter.set_parameter(&params.topology, CircuitTopology::Fun);
                            setter.end_set_parameter(&params.topology);

                            setter.begin_set_parameter(&params.reso_comp);
                            setter.set_parameter_normalized(&params.reso_comp, 0.5);
                            setter.end_set_parameter(&params.reso_comp);
                        }
                    });
                } else {
                    // Row of 9 MEDIUM knobs filling the full 50 px body, top-aligned
                    body_ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                        let total_w = ui.available_width();
                        let col_w = total_w / 9.0;
                        let col_h = ui.available_height();

                        let mut render_knob = |knob: Knob303<'_, _>| {
                            ui.allocate_ui_with_layout(
                                vec2(col_w, col_h),
                                Layout::top_down(Align::Center),
                                |ui| {
                                    ui.add(knob);
                                },
                            );
                        };

                        render_knob(
                            Knob303::new(
                                &params.vcf_drive,
                                setter,
                                "DRIVE",
                                theme::KNOB_RADIUS_MEDIUM,
                            )
                            .tooltip("VCF input drive (dB)"),
                        );
                        render_knob(
                            Knob303::new(&params.sub_vol, setter, "SUB", theme::KNOB_RADIUS_MEDIUM)
                                .tooltip("Sub-oscillator level"),
                        );
                        render_knob(
                            Knob303::new(&params.glide, setter, "GLIDE", theme::KNOB_RADIUS_MEDIUM)
                                .value_text(crate::value_text::glide(
                                    params.glide.unmodulated_plain_value(),
                                    params.topology.value(),
                                    params.advanced_env.value(),
                                ))
                                .tooltip("Pitch glide time on slid notes"),
                        );
                        render_knob(
                            Knob303::new(
                                &params.osc1_pitch,
                                setter,
                                "PITCH1",
                                theme::KNOB_RADIUS_MEDIUM,
                            )
                            .tooltip("Oscillator 1 pitch offset (semitones)"),
                        );
                        render_knob(
                            Knob303::new(
                                &params.osc2_pitch,
                                setter,
                                "PITCH2",
                                theme::KNOB_RADIUS_MEDIUM,
                            )
                            .tooltip("Oscillator 2 pitch offset (semitones)"),
                        );
                        render_knob(
                            Knob303::new(
                                &params.osc2_vol,
                                setter,
                                "MIX2",
                                theme::KNOB_RADIUS_MEDIUM,
                            )
                            .tooltip("Oscillator 2 level"),
                        );
                        render_knob(
                            Knob303::new(&params.osc1_pw, setter, "PW", theme::KNOB_RADIUS_MEDIUM)
                                .tooltip("Oscillator 1 pulse width"),
                        );
                        render_knob(
                            Knob303::new(
                                &params.sweep_pos,
                                setter,
                                "SWEEP",
                                theme::KNOB_RADIUS_MEDIUM,
                            )
                            .tooltip("Oscillator 1 waveform morph, saw to square"),
                        );
                        render_knob(
                            Knob303::new(
                                &params.reso_comp,
                                setter,
                                "R-COMP",
                                theme::KNOB_RADIUS_MEDIUM,
                            )
                            .tooltip(
                                "Resonance gain compensation: makes up the level lost at high \
                                 resonance",
                            ),
                        );
                    });
                }
            },
        );
}
