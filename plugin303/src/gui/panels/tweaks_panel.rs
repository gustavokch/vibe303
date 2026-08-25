use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{vec2, ComboBox, CornerRadius, DragValue, FontId, Frame, Margin, Ui};

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

                    // Top row: Tolerance, Age, Noise, Seed
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(6.0, 0.0);

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

                        ui.label(
                            nih_plug_egui::egui::RichText::new("AGE:")
                                .font(FontId::proportional(8.5))
                                .color(label_color),
                        );
                        let mut age_val = params.comp_age.value();
                        if ui
                            .add(
                                DragValue::new(&mut age_val)
                                    .range(0..=100)
                                    .custom_formatter(|v, _| format!("{:.0} yrs", v)),
                            )
                            .changed()
                        {
                            setter.begin_set_parameter(&params.comp_age);
                            setter.set_parameter(&params.comp_age, age_val);
                            setter.end_set_parameter(&params.comp_age);
                        }

                        ui.label(
                            nih_plug_egui::egui::RichText::new("NOISE:")
                                .font(FontId::proportional(8.5))
                                .color(label_color),
                        );
                        let mut noise_val = params.thermal_noise.value();
                        if ui
                            .add(
                                DragValue::new(&mut noise_val)
                                    .range(0..=100)
                                    .custom_formatter(|v, _| format!("{:.0}%", v)),
                            )
                            .changed()
                        {
                            setter.begin_set_parameter(&params.thermal_noise);
                            setter.set_parameter(&params.thermal_noise, noise_val);
                            setter.end_set_parameter(&params.thermal_noise);
                        }

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

                    ui.add_space(4.0);

                    // Bottom row: Drift and Recal knobs aligned under upper controls
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(24.0, 0.0);
                        ui.add_space(4.0);
                        ui.add(Knob303::new(&params.comp_drift, setter, "DRIFT", 11.5));
                        ui.add(Knob303::new(
                            &params.recalibration_scale,
                            setter,
                            "RECAL",
                            11.5,
                        ));
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
                        // Strikethrough marks the button as inert in the idle
                        // state (audit #19): calibration is only meaningful
                        // when needs_rebuild flags drifted params.
                        let btn_text = {
                            let text = nih_plug_egui::egui::RichText::new(if calibrating {
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
                            if !calibrating { text.strikethrough() } else { text }
                        };
                        if ui.add(nih_plug_egui::egui::Button::new(btn_text).fill(btn_color)).clicked() {
                            setter.begin_set_parameter(&params.calibrate_sync);
                            setter.set_parameter(&params.calibrate_sync, true);
                            setter.end_set_parameter(&params.calibrate_sync);
                        }
                    });
                });
            });
        });
}
