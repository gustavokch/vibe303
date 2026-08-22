use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{vec2, ComboBox, CornerRadius, DragValue, FontId, Frame, Margin, Ui};

use crate::gui::theme;
use crate::gui::widgets::Knob303;
use crate::params::{CircuitTopology, CompTolerance, Plugin303Params};

pub fn render_tweaks_panel(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let is_accurate = params.topology.value() == CircuitTopology::Accurate;

    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::symmetric(6, 6))
        .show(ui, |ui| {
            ui.add_enabled_ui(!is_accurate, |ui| {
                ui.vertical(|ui| {
                    ui.label(
                        nih_plug_egui::egui::RichText::new("ANALOG CHARACTER & TWEAKS")
                            .font(FontId::proportional(10.0))
                            .strong()
                            .color(if is_accurate {
                                theme::TEXT_MUTED
                            } else {
                                theme::TEXT_PRIMARY
                            }),
                    );

                    ui.add_space(3.0);

                    // Top row: Tolerance, Age, Noise, Seed
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(6.0, 0.0);

                        ui.label(
                            nih_plug_egui::egui::RichText::new("TOL:")
                                .font(FontId::proportional(8.5))
                                .color(theme::TEXT_SECONDARY),
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
                                .color(theme::TEXT_SECONDARY),
                        );
                        let mut age_val = params.comp_age.value();
                        if ui.add(DragValue::new(&mut age_val).range(0..=100)).changed() {
                            setter.begin_set_parameter(&params.comp_age);
                            setter.set_parameter(&params.comp_age, age_val);
                            setter.end_set_parameter(&params.comp_age);
                        }

                        ui.label(
                            nih_plug_egui::egui::RichText::new("NOISE:")
                                .font(FontId::proportional(8.5))
                                .color(theme::TEXT_SECONDARY),
                        );
                        let mut noise_val = params.thermal_noise.value();
                        if ui
                            .add(DragValue::new(&mut noise_val).range(0..=100))
                            .changed()
                        {
                            setter.begin_set_parameter(&params.thermal_noise);
                            setter.set_parameter(&params.thermal_noise, noise_val);
                            setter.end_set_parameter(&params.thermal_noise);
                        }

                        ui.label(
                            nih_plug_egui::egui::RichText::new("SEED:")
                                .font(FontId::proportional(8.5))
                                .color(theme::TEXT_SECONDARY),
                        );
                        let mut seed_val = params.comp_seed.value();
                        if ui
                            .add(DragValue::new(&mut seed_val).range(0..=9999))
                            .changed()
                        {
                            setter.begin_set_parameter(&params.comp_seed);
                            setter.set_parameter(&params.comp_seed, seed_val);
                            setter.end_set_parameter(&params.comp_seed);
                        }
                    });

                    ui.add_space(2.0);

                    // Bottom row: Drift and Recal knobs
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(16.0, 0.0);
                        ui.add_space(6.0);
                        ui.add(Knob303::new(&params.comp_drift, setter, "DRIFT", 11.5));
                        ui.add(Knob303::new(
                            &params.recalibration_scale,
                            setter,
                            "RECAL",
                            11.5,
                        ));
                    });
                });
            });
        });
}
