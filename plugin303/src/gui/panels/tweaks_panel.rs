use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{pos2, vec2, ComboBox, CornerRadius, DragValue, FontId, Frame, Margin, Sense, Shape, Stroke, Ui};

use crate::gui::theme;
use crate::gui::widgets::Knob303;
use crate::params::{CircuitTopology, CompTolerance, Plugin303Params};

pub fn render_tweaks_panel(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let is_accurate = params.topology.value() == CircuitTopology::Accurate;
    let label_color = if is_accurate {
        theme::TEXT_MUTED
    } else {
        theme::TEXT_SECONDARY
    };

    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::same(6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(95.0);
            ui.add_enabled_ui(!is_accurate, |ui| {
                ui.vertical(|ui| {
                    ui.label(
                        nih_plug_egui::egui::RichText::new("ANALOG CHARACTER & TWEAKS")
                            .font(FontId::proportional(10.0))
                            .strong()
                            .color(if is_accurate {
                                theme::TEXT_MUTED
                            } else {
                                theme::ACCENT_ORANGE
                            }),
                    );

                    ui.add_space(3.0);

                    // Top row: Tolerance, Age, Noise, Seed distributed evenly
                    // across the panel width so no dead band remains.
                    ui.columns(4, |cols| {
                        cols[0].vertical_centered(|ui| {
                            ui.label(
                                nih_plug_egui::egui::RichText::new("TOL:")
                                    .font(FontId::proportional(8.5))
                                    .color(label_color),
                            );
                            let tol = params.tolerance.value();
                            ComboBox::from_id_salt("tweaks_tolerance_combo")
                                .selected_text(match tol {
                                    CompTolerance::Zero => "0%",
                                    CompTolerance::PointFive => "0.5%",
                                    CompTolerance::Two => "2%",
                                    CompTolerance::Five => "5%",
                                    CompTolerance::Ten => "10%",
                                })
                                .width(44.0)
                                .show_ui(ui, |ui| {
                                    for (variant, label) in [
                                        (CompTolerance::Zero, "0%"),
                                        (CompTolerance::PointFive, "0.5%"),
                                        (CompTolerance::Two, "2%"),
                                        (CompTolerance::Five, "5%"),
                                        (CompTolerance::Ten, "10%"),
                                    ] {
                                        if ui.selectable_label(tol == variant, label).clicked() {
                                            setter.begin_set_parameter(&params.tolerance);
                                            setter.set_parameter(&params.tolerance, variant);
                                            setter.end_set_parameter(&params.tolerance);
                                        }
                                    }
                                });
                        });

                        cols[1].vertical_centered(|ui| {
                            ui.label(
                                nih_plug_egui::egui::RichText::new("AGE:")
                                    .font(FontId::proportional(8.5))
                                    .color(label_color),
                            );
                            ui.horizontal(|ui| {
                                let mut age_val = params.comp_age.value();
                                let drag_resp = ui.add(
                                    DragValue::new(&mut age_val)
                                        .range(0..=100)
                                        .custom_formatter(|v, _| format!("{:.0} yrs", v)),
                                );
                                if drag_resp.changed() {
                                    setter.begin_set_parameter(&params.comp_age);
                                    setter.set_parameter(&params.comp_age, age_val);
                                    setter.end_set_parameter(&params.comp_age);
                                }
                                ui.add_space(2.0);
                                let (arrow_rect, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                                if ui.is_rect_visible(arrow_rect) {
                                    let c = arrow_rect.center();
                                    let tri = vec![
                                        pos2(c.x - 3.0, c.y - 2.0),
                                        pos2(c.x + 3.0, c.y - 2.0),
                                        pos2(c.x, c.y + 3.0),
                                    ];
                                    ui.painter().add(Shape::convex_polygon(
                                        tri,
                                        label_color,
                                        Stroke::NONE,
                                    ));
                                }
                            });
                        });

                        cols[2].vertical_centered(|ui| {
                            ui.label(
                                nih_plug_egui::egui::RichText::new("NOISE:")
                                    .font(FontId::proportional(8.5))
                                    .color(label_color),
                            );
                            ui.horizontal(|ui| {
                                let mut noise_val = params.thermal_noise.value();
                                let drag_resp = ui.add(
                                    DragValue::new(&mut noise_val)
                                        .range(0..=100)
                                        .custom_formatter(|v, _| format!("{:.0}%", v)),
                                );
                                if drag_resp.changed() {
                                    setter.begin_set_parameter(&params.thermal_noise);
                                    setter.set_parameter(&params.thermal_noise, noise_val);
                                    setter.end_set_parameter(&params.thermal_noise);
                                }
                                ui.add_space(2.0);
                                let (arrow_rect, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                                if ui.is_rect_visible(arrow_rect) {
                                    let c = arrow_rect.center();
                                    let tri = vec![
                                        pos2(c.x - 3.0, c.y - 2.0),
                                        pos2(c.x + 3.0, c.y - 2.0),
                                        pos2(c.x, c.y + 3.0),
                                    ];
                                    ui.painter().add(Shape::convex_polygon(
                                        tri,
                                        label_color,
                                        Stroke::NONE,
                                    ));
                                }
                            });
                        });

                        cols[3].vertical_centered(|ui| {
                            ui.label(
                                nih_plug_egui::egui::RichText::new("COMP SEED:")
                                    .font(FontId::proportional(8.5))
                                    .color(label_color),
                            );
                            let mut seed_val = params.comp_seed.value();
                            if ui
                                .add(
                                    DragValue::new(&mut seed_val)
                                        .range(0..=9999)
                                        .custom_formatter(|v, _| format!("{:04}", v as u32)),
                                )
                                .changed()
                            {
                                setter.begin_set_parameter(&params.comp_seed);
                                setter.set_parameter(&params.comp_seed, seed_val);
                                setter.end_set_parameter(&params.comp_seed);
                            }
                        });
                    });

                    ui.add_space(4.0);

                    // Bottom row: Drift and Recal knobs + Calibrate button
                    // distributed evenly across the panel width.
                    ui.columns(3, |cols| {
                        cols[0].vertical_centered(|ui| {
                            ui.add(Knob303::new(&params.comp_drift,
                                setter,
                                "DRIFT",
                                11.5,
                            ));
                        });
                        cols[1].vertical_centered(|ui| {
                            ui.add(Knob303::new(
                                &params.recalibration_scale,
                                setter,
                                "RECAL",
                                11.5,
                            ));
                        });
                        cols[2].vertical_centered(|ui| {
                            // Calibrate sync trigger — shaded when idle, lights when aging/recal changes need rebuild
                            let needs_rebuild = params.recalibration_scale.value() != 1.0
                                || params.comp_age.value() != 0
                                || params.tolerance.value() != crate::params::CompTolerance::Zero
                                || params.thermal_noise.value() != 0
                                || params.comp_drift.value() != 0.1;
                            let calibrating = params.calibrate_sync.value();
                            let btn_color = if calibrating {
                                theme::ACCENT_CYAN
                            } else if needs_rebuild {
                                theme::ACCENT_ORANGE
                            } else {
                                theme::WELL_BG
                            };
                            // Idle state uses dimmed text + border instead of
                            // strikethrough so it reads as inert, not deprecated.
                            let btn_text = nih_plug_egui::egui::RichText::new(if calibrating {
                                "CALIBRATING..."
                            } else {
                                "CALIBRATE"
                            })
                            .font(FontId::proportional(8.0))
                            .color(if calibrating {
                                theme::TEXT_ON_ACCENT
                            } else if needs_rebuild {
                                theme::TEXT_PRIMARY
                            } else {
                                theme::TEXT_MUTED
                            });
                            let mut cal_btn = nih_plug_egui::egui::Button::new(btn_text).fill(btn_color);
                            if !calibrating && !needs_rebuild {
                                cal_btn = cal_btn.stroke(Stroke::new(1.0_f32, theme::BORDER_DEFAULT));
                            }
                            if ui.add(cal_btn).clicked() {
                                setter.begin_set_parameter(&params.calibrate_sync);
                                setter.set_parameter(&params.calibrate_sync, true);
                                setter.end_set_parameter(&params.calibrate_sync);
                            }
                        });
                    });
                });
            });
        });
}
