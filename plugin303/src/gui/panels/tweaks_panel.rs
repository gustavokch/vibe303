use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    pos2, vec2, Align, Align2, Button, ComboBox, DragValue, FontId, Layout, RichText, Sense, Ui,
};

use crate::gui::theme;
use crate::gui::widgets::panel::DevicePanel;
use crate::gui::widgets::Knob303;
use crate::params::{CircuitTopology, CompTolerance, Plugin303Params};

pub fn render_tweaks_panel(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let is_accurate = params.topology.value() == CircuitTopology::Accurate;
    let calibrating = params.calibrate_sync.value();
    let needs_rebuild = params.recalibration_scale.value() != 1.0
        || params.comp_age.value() != 0
        || params.tolerance.value() != crate::params::CompTolerance::Zero
        || params.thermal_noise.value() != 0
        || params.comp_drift.value() != 0.1;

    DevicePanel::new("ANALOG TWEAKS", crate::gui::TWEAKS_H)
        .dimmed(is_accurate)
        .show(
            ui,
            |header_ui| {
                header_ui.add_enabled_ui(!is_accurate, |ui| {
                    let btn_text = if calibrating {
                        "CALIBRATING..."
                    } else {
                        "CALIBRATE"
                    };

                    let mut cal_btn = Button::new(
                        RichText::new(btn_text)
                            .font(FontId::proportional(8.5))
                            .color(if needs_rebuild || calibrating {
                                theme::TEXT_ON_ACCENT
                            } else {
                                theme::TEXT_PRIMARY
                            }),
                    )
                    .min_size(vec2(80.0, crate::gui::widgets::toggle::TOGGLE_H));

                    if needs_rebuild || calibrating {
                        cal_btn = cal_btn.fill(theme::ACCENT_ORANGE);
                    }

                    if ui
                        .add(cal_btn)
                        .on_hover_text("Rebuild analog model with current component tolerances")
                        .clicked()
                    {
                        setter.begin_set_parameter(&params.calibrate_sync);
                        setter.set_parameter(&params.calibrate_sync, true);
                        setter.end_set_parameter(&params.calibrate_sync);
                    }
                });
            },
            |body_ui| {
                body_ui.add_enabled_ui(!is_accurate, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                        let total_w = ui.available_width();
                        let total_h = ui.available_height();

                        let knob_size = Knob303::<nih_plug::prelude::FloatParam>::size(
                            theme::KNOB_RADIUS_LARGE,
                        );
                        // Field column: 60 px label + the widest field (the TOL combo).
                        const FIELD_COL_W: f32 = 172.0;
                        let col_gap = 8.0;
                        let left_w = FIELD_COL_W.min(total_w);
                        let right_w = (total_w - left_w - col_gap).max(0.0);

                        // Left column: 4 label-left field rows + 1 calibration status row
                        ui.allocate_ui_with_layout(
                            vec2(left_w, total_h),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_min_size(vec2(left_w, total_h));
                                ui.scope(|ui| {
                                    theme::field_style(ui);

                                    const LABEL_W: f32 = 60.0;
                                    let row_h = crate::gui::widgets::toggle::TOGGLE_H;
                                    let status_h = 14.0;
                                    let content_h = 4.0 * row_h + status_h;
                                    // Even gaps: above the first row, between rows, below the status.
                                    let v_gap = ((total_h - content_h) / 6.0).max(3.0);

                                    let label_color = theme::TEXT_SECONDARY;
                                    let label_font = FontId::proportional(8.5);

                                    let field_row = |ui: &mut Ui,
                                                     label: &str,
                                                     add_content: &mut dyn FnMut(&mut Ui)| {
                                        ui.horizontal(|ui| {
                                            let (rect, _) = ui.allocate_exact_size(
                                                vec2(LABEL_W, row_h),
                                                Sense::hover(),
                                            );
                                            ui.painter().text(
                                                pos2(rect.left(), rect.center().y),
                                                Align2::LEFT_CENTER,
                                                label,
                                                label_font.clone(),
                                                label_color,
                                            );
                                            add_content(ui);
                                        });
                                    };

                                    ui.add_space(v_gap);

                                    // TOL
                                    field_row(ui, "TOL", &mut |ui| {
                                        let tol = params.tolerance.value();
                                        ComboBox::from_id_salt("tweaks_tolerance_combo")
                                            .selected_text(
                                                RichText::new(match tol {
                                                    CompTolerance::Zero => "0%",
                                                    CompTolerance::PointFive => "0.5%",
                                                    CompTolerance::Two => "2%",
                                                    CompTolerance::Five => "5%",
                                                    CompTolerance::Ten => "10%",
                                                })
                                                .font(FontId::proportional(8.5)),
                                            )
                                            .width(80.0)
                                            .show_ui(ui, |ui| {
                                                for (variant, opt_label) in [
                                                    (CompTolerance::Zero, "0% (Ideal)"),
                                                    (CompTolerance::PointFive, "0.5% (Tight)"),
                                                    (CompTolerance::Two, "2% (Stock)"),
                                                    (CompTolerance::Five, "5% (Loose)"),
                                                    (CompTolerance::Ten, "10% (Wild)"),
                                                ] {
                                                    if ui
                                                        .selectable_label(
                                                            tol == variant,
                                                            RichText::new(opt_label)
                                                                .font(FontId::proportional(8.5)),
                                                        )
                                                        .clicked()
                                                    {
                                                        setter
                                                            .begin_set_parameter(&params.tolerance);
                                                        setter.set_parameter(
                                                            &params.tolerance,
                                                            variant,
                                                        );
                                                        setter.end_set_parameter(&params.tolerance);
                                                    }
                                                }
                                            });
                                    });

                                    ui.add_space(v_gap);

                                    // AGE
                                    field_row(ui, "AGE", &mut |ui| {
                                        let mut age_val = params.comp_age.value();
                                        if ui
                                            .add(
                                                DragValue::new(&mut age_val)
                                                    .range(0..=100)
                                                    .speed(0.2)
                                                    .custom_formatter(|v, _| {
                                                        format!("{:.0} yrs", v)
                                                    }),
                                            )
                                            .on_hover_text("Component Aging (0-100 years drag)")
                                            .changed()
                                        {
                                            setter.begin_set_parameter(&params.comp_age);
                                            setter.set_parameter(&params.comp_age, age_val);
                                            setter.end_set_parameter(&params.comp_age);
                                        }
                                    });

                                    ui.add_space(v_gap);

                                    // NOISE
                                    field_row(ui, "NOISE", &mut |ui| {
                                        let mut noise_val = params.thermal_noise.value();
                                        if ui
                                            .add(
                                                DragValue::new(&mut noise_val)
                                                    .range(0..=100)
                                                    .speed(0.2)
                                                    .custom_formatter(|v, _| format!("{:.0}%", v)),
                                            )
                                            .on_hover_text("Thermal Noise floor (0-100% drag)")
                                            .changed()
                                        {
                                            setter.begin_set_parameter(&params.thermal_noise);
                                            setter.set_parameter(&params.thermal_noise, noise_val);
                                            setter.end_set_parameter(&params.thermal_noise);
                                        }
                                    });

                                    ui.add_space(v_gap);

                                    // CMP SEED
                                    field_row(ui, "CMP SEED", &mut |ui| {
                                        let mut seed_val = params.comp_seed.value();
                                        if ui
                                            .add(
                                                DragValue::new(&mut seed_val)
                                                    .range(0..=9999)
                                                    .speed(1.0)
                                                    .custom_formatter(|v, _| {
                                                        format!("{:04}", v as u32)
                                                    }),
                                            )
                                            .on_hover_text(
                                                "Component Variation Seed (0000-9999 drag)",
                                            )
                                            .changed()
                                        {
                                            setter.begin_set_parameter(&params.comp_seed);
                                            setter.set_parameter(&params.comp_seed, seed_val);
                                            setter.end_set_parameter(&params.comp_seed);
                                        }
                                    });

                                    ui.add_space(v_gap);

                                    // Fifth line: Calibration status under fields
                                    let status_text = if needs_rebuild {
                                        "SYNC PENDING"
                                    } else {
                                        "CALIBRATED"
                                    };
                                    ui.horizontal(|ui| {
                                        ui.add_space(LABEL_W);
                                        ui.label(
                                            RichText::new(status_text)
                                                .font(label_font.clone())
                                                .color(if needs_rebuild {
                                                    theme::ACCENT_ORANGE
                                                } else {
                                                    theme::TEXT_SECONDARY
                                                }),
                                        );
                                    });
                                });
                            },
                        );

                        ui.add_space(col_gap);

                        // Right column: DRIFT and RECAL as LARGE knobs, each centred in
                        // an equal half of the free width, vertically centred.
                        let knob_col_w = right_w * 0.5;
                        let pad_y = ((total_h - knob_size.y) * 0.5).max(0.0);
                        let knob_column = |ui: &mut Ui, add_knob: &mut dyn FnMut(&mut Ui)| {
                            ui.allocate_ui_with_layout(
                                vec2(knob_col_w, total_h),
                                Layout::top_down(Align::Center),
                                |ui| {
                                    ui.set_min_size(vec2(knob_col_w, total_h));
                                    ui.add_space(pad_y);
                                    add_knob(ui);
                                },
                            );
                        };
                        knob_column(ui, &mut |ui| {
                            ui.add(Knob303::new(
                                &params.comp_drift,
                                setter,
                                "DRIFT",
                                theme::KNOB_RADIUS_LARGE,
                            ));
                        });
                        knob_column(ui, &mut |ui| {
                            ui.add(Knob303::new(
                                &params.recalibration_scale,
                                setter,
                                "RECAL",
                                theme::KNOB_RADIUS_LARGE,
                            ));
                        });
                    });
                });
            },
        );
}
