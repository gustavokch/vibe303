use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    pos2, vec2, Align, Align2, Button, Color32, ComboBox, DragValue, FontId, Layout, Rect,
    RichText, Sense, Ui,
};

use crate::gui::theme;
use crate::gui::widgets::panel::DevicePanel;
use crate::gui::widgets::{Knob303, Readout};
use crate::params::{CircuitTopology, CompTolerance, Plugin303Params, VcoTrim};

/// Width of every value in the field column: the TOL combo's width, so the
/// four fields share one right edge (round-20 #7).
pub const FIELD_W: f32 = 80.0;

/// Hover text of every ANALOG TWEAKS control in ACCURATE (round-20 #3).
pub const ACCURATE_HINT: &str =
    "Analog tweaks apply to FUN mode only. Set TOPOLOGY to FUN to edit them.";

pub fn render_tweaks_panel(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let is_accurate = params.topology.value() == CircuitTopology::Accurate;
    // A VCO trim is being computed in the background (after CALIBRATE, a
    // topology switch or a project load).
    let calibrating = params
        .vco_trim_busy
        .load(std::sync::atomic::Ordering::Relaxed);
    let trimmed_for = match params.vco_trim.read() {
        Ok(trim) => *trim,
        Err(poisoned) => *poisoned.into_inner(),
    };
    // AGE / TOL / SEED / RECAL moved since the last CALIBRATE: the live
    // components no longer match the trim, so the pitch is off until the next.
    let needs_rebuild = VcoTrim::from_params(params) != trimmed_for.normalized();

    DevicePanel::new("ANALOG TWEAKS", crate::gui::TWEAKS_H)
        .dimmed(is_accurate)
        .show(
            ui,
            |header_ui| {
                if is_accurate {
                    return;
                }
                // Lit only as the call to action of a due rebuild. A trim that
                // runs in the background is not a call to action: the tile
                // stays raised and the status line alone says CALIBRATING
                // (round-22 #4).
                let cal_btn = Button::new(
                    RichText::new("CALIBRATE")
                        .font(FontId::proportional(8.5))
                        .color(if needs_rebuild {
                            theme::TEXT_ON_ACCENT
                        } else {
                            theme::TEXT_PRIMARY
                        }),
                )
                .min_size(vec2(80.0, crate::gui::widgets::toggle::TOGGLE_H));
                let cal_resp = header_ui
                    .scope(|ui| {
                        if needs_rebuild {
                            theme::lit_action_style(ui);
                        }
                        ui.add(cal_btn)
                    })
                    .inner;

                if cal_resp
                    .on_hover_text(
                        "Calibrate the VCO pitch for the current AGE, TOL, CMP SEED and \
                         RECAL. Runs in the background",
                    )
                    .clicked()
                {
                    if let Ok(mut trim) = params.vco_trim.write() {
                        *trim = VcoTrim::from_params(params);
                    }
                }
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

                                    // Labels are painted, not egui Labels, so the
                                    // disabled group cannot fade them (round-20 #1).
                                    let label_color = if is_accurate {
                                        theme::TEXT_MUTED
                                    } else {
                                        theme::TEXT_SECONDARY
                                    };
                                    let label_font = FontId::proportional(8.5);
                                    let paint_left =
                                        |ui: &Ui, rect: Rect, text: &str, color: Color32| {
                                            let mut painter = ui.painter().clone();
                                            painter.set_fade_to_color(None);
                                            painter.text(
                                                pos2(rect.left(), rect.center().y),
                                                Align2::LEFT_CENTER,
                                                text,
                                                label_font.clone(),
                                                color,
                                            );
                                        };

                                    let field_row = |ui: &mut Ui,
                                                     label: &str,
                                                     add_content: &mut dyn FnMut(&mut Ui)| {
                                        ui.horizontal(|ui| {
                                            let (rect, _) = ui.allocate_exact_size(
                                                vec2(LABEL_W, row_h),
                                                Sense::hover(),
                                            );
                                            paint_left(ui, rect, label, label_color);
                                            add_content(ui);
                                        });
                                    };

                                    ui.add_space(v_gap);

                                    // TOL
                                    field_row(ui, "TOL", &mut |ui| {
                                        let tol = params.tolerance.value();
                                        let tol_text = match tol {
                                            CompTolerance::Zero => "0%",
                                            CompTolerance::PointFive => "0.5%",
                                            CompTolerance::Two => "2%",
                                            CompTolerance::Five => "5%",
                                            CompTolerance::Ten => "10%",
                                        };
                                        if is_accurate {
                                            ui.add(Readout::new(tol_text, FIELD_W, row_h))
                                                .on_disabled_hover_text(ACCURATE_HINT);
                                            return;
                                        }
                                        // The value is a number: 9 pt monospace, centred
                                        // like the DragValues under it (round-22 #11).
                                        // egui paints a combo's text left-aligned in the
                                        // Button face, so the combo shows none and the
                                        // value is painted over it.
                                        let combo =
                                            ComboBox::from_id_salt("tweaks_tolerance_combo")
                                                .selected_text("")
                                                .width(FIELD_W)
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
                                                                opt_label,
                                                            )
                                                            .clicked()
                                                        {
                                                            setter.begin_set_parameter(
                                                                &params.tolerance,
                                                            );
                                                            setter.set_parameter(
                                                                &params.tolerance,
                                                                variant,
                                                            );
                                                            setter.end_set_parameter(
                                                                &params.tolerance,
                                                            );
                                                        }
                                                    }
                                                });
                                        ui.painter().text(
                                            combo.response.rect.center(),
                                            Align2::CENTER_CENTER,
                                            tol_text,
                                            FontId::monospace(9.0),
                                            theme::TEXT_PRIMARY,
                                        );
                                        combo.response.on_hover_text(
                                            "Component tolerance: how far part values stray \
                                             from nominal",
                                        );
                                    });

                                    ui.add_space(v_gap);

                                    // AGE
                                    field_row(ui, "AGE", &mut |ui| {
                                        let age = params.comp_age.value();
                                        if is_accurate {
                                            ui.add(Readout::new(
                                                &format!("{age} yrs"),
                                                FIELD_W,
                                                row_h,
                                            ))
                                            .on_disabled_hover_text(ACCURATE_HINT);
                                            return;
                                        }
                                        let mut age_val = age;
                                        if ui
                                            .add_sized(
                                                vec2(FIELD_W, row_h),
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
                                        let noise = params.thermal_noise.value();
                                        if is_accurate {
                                            ui.add(Readout::new(
                                                &format!("{noise}%"),
                                                FIELD_W,
                                                row_h,
                                            ))
                                            .on_disabled_hover_text(ACCURATE_HINT);
                                            return;
                                        }
                                        let mut noise_val = noise;
                                        if ui
                                            .add_sized(
                                                vec2(FIELD_W, row_h),
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
                                        let seed = params.comp_seed.value();
                                        if is_accurate {
                                            ui.add(Readout::new(
                                                &format!("{seed:04}"),
                                                FIELD_W,
                                                row_h,
                                            ))
                                            .on_disabled_hover_text(ACCURATE_HINT);
                                            return;
                                        }
                                        let mut seed_val = seed;
                                        if ui
                                            .add_sized(
                                                vec2(FIELD_W, row_h),
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

                                    // Status under the fields. In ACCURATE it says the
                                    // card is FUN-only (round-20 #3). Every other state
                                    // names the CALIBRATE action (round-22 #24).
                                    let (status_text, status_color) = if is_accurate {
                                        ("FUN MODE ONLY", theme::TEXT_MUTED)
                                    } else if needs_rebuild {
                                        ("CALIBRATION DUE", theme::ACCENT_ORANGE)
                                    } else if calibrating {
                                        ("CALIBRATING", theme::TEXT_SECONDARY)
                                    } else {
                                        ("CALIBRATED", theme::TEXT_SECONDARY)
                                    };
                                    ui.horizontal(|ui| {
                                        ui.add_space(LABEL_W);
                                        let (rect, _) = ui.allocate_exact_size(
                                            vec2(ui.available_width(), status_h),
                                            Sense::hover(),
                                        );
                                        paint_left(ui, rect, status_text, status_color);
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
                            ui.add(
                                Knob303::new(
                                    &params.comp_drift,
                                    setter,
                                    "DRIFT",
                                    theme::KNOB_RADIUS_LARGE,
                                )
                                .tooltip(
                                    "Component drift: slow random wander of pitch, cutoff and \
                                     resonance",
                                ),
                            )
                            .on_disabled_hover_text(ACCURATE_HINT);
                        });
                        knob_column(ui, &mut |ui| {
                            ui.add(
                                Knob303::new(
                                    &params.recalibration_scale,
                                    setter,
                                    "RECAL",
                                    theme::KNOB_RADIUS_LARGE,
                                )
                                .tooltip(
                                    "Recalibration: share of the component pitch error the next \
                                     CALIBRATE removes",
                                ),
                            )
                            .on_disabled_hover_text(ACCURATE_HINT);
                        });
                    });
                });
            },
        );
}
