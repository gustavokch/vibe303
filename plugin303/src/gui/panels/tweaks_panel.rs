use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    vec2, Align, ComboBox, CornerRadius, DragValue, FontId, Frame, Layout, Margin, Stroke, Ui,
};

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

                    // 4 Balanced Columns across full available width
                    const NUM_COLS: f32 = 4.0;
                    const NUM_SEPARATORS: f32 = NUM_COLS - 1.0;
                    const SEPARATOR_WIDTH: f32 = 6.0;
                    const ITEM_SPACING_X: f32 = 8.0;
                    // Each vertical separator allocates 6px and has 8px item_spacing on both sides
                    const SEPARATOR_OVERHEAD: f32 =
                        NUM_SEPARATORS * (SEPARATOR_WIDTH + 2.0 * ITEM_SPACING_X); // 66.0 px total

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(ITEM_SPACING_X, 0.0);
                        let total_w = ui.available_width();
                        let col_w = (total_w - SEPARATOR_OVERHEAD) / NUM_COLS;

                        // Column 1: Component Tolerance & Aging
                        ui.allocate_ui_with_layout(
                            vec2(col_w, 64.0),
                            Layout::top_down(Align::Center),
                            |ui| {
                                ui.set_min_width(col_w);
                                ui.set_width(col_w);

                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
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
                                        .width(52.0)
                                        .show_ui(ui, |ui| {
                                            for (variant, label) in [
                                                (CompTolerance::Zero, "0% (Ideal)"),
                                                (CompTolerance::PointFive, "0.5% (Tight)"),
                                                (CompTolerance::Two, "2% (Stock)"),
                                                (CompTolerance::Five, "5% (Loose)"),
                                                (CompTolerance::Ten, "10% (Wild)"),
                                            ] {
                                                if ui
                                                    .selectable_label(tol == variant, label)
                                                    .clicked()
                                                {
                                                    setter.begin_set_parameter(&params.tolerance);
                                                    setter
                                                        .set_parameter(&params.tolerance, variant);
                                                    setter.end_set_parameter(&params.tolerance);
                                                }
                                            }
                                        });
                                });

                                ui.add_space(2.0);

                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
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
                                                .speed(0.2)
                                                .custom_formatter(|v, _| format!("{:.0} yrs", v)),
                                        )
                                        .on_hover_text("Component Aging (0-100 years drag)")
                                        .changed()
                                    {
                                        setter.begin_set_parameter(&params.comp_age);
                                        setter.set_parameter(&params.comp_age, age_val);
                                        setter.end_set_parameter(&params.comp_age);
                                    }

                                    ui.add_space(6.0);

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
                            },
                        );

                        ui.separator();

                        // Column 2: Drift & Instability
                        ui.allocate_ui_with_layout(
                            vec2(col_w, 64.0),
                            Layout::top_down(Align::Center),
                            |ui| {
                                ui.set_min_width(col_w);
                                ui.set_width(col_w);

                                // Invisible placeholder matching column 3's SEED row so DRIFT and RECAL align by construction
                                ui.horizontal(|ui| {
                                    ui.set_invisible();
                                    ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                                    ui.label(
                                        nih_plug_egui::egui::RichText::new("SEED:")
                                            .font(FontId::proportional(8.5)),
                                    );
                                    let mut placeholder = 0.0_f32;
                                    ui.add(DragValue::new(&mut placeholder));
                                });
                                ui.add_space(2.0);

                                ui.add(Knob303::new(&params.comp_drift, setter, "DRIFT", 11.0));
                            },
                        );

                        ui.separator();

                        // Column 3: Seed & Recalibration Scale
                        ui.allocate_ui_with_layout(
                            vec2(col_w, 64.0),
                            Layout::top_down(Align::Center),
                            |ui| {
                                ui.set_min_width(col_w);
                                ui.set_width(col_w);

                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                                    ui.label(
                                        nih_plug_egui::egui::RichText::new("SEED:")
                                            .font(FontId::proportional(8.5))
                                            .color(label_color),
                                    );
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
                                        .on_hover_text("Component Variation Seed (0000-9999 drag)")
                                        .changed()
                                    {
                                        setter.begin_set_parameter(&params.comp_seed);
                                        setter.set_parameter(&params.comp_seed, seed_val);
                                        setter.end_set_parameter(&params.comp_seed);
                                    }
                                });

                                ui.add_space(2.0);

                                ui.add(Knob303::new(
                                    &params.recalibration_scale,
                                    setter,
                                    "RECAL",
                                    11.0,
                                ));
                            },
                        );

                        ui.separator();

                        // Column 4: Calibration Action & Status
                        ui.allocate_ui_with_layout(
                            vec2(col_w, 64.0),
                            Layout::top_down(Align::Center),
                            |ui| {
                                ui.set_min_width(col_w);
                                ui.set_width(col_w);

                                let needs_rebuild = params.recalibration_scale.value() != 1.0
                                    || params.comp_age.value() != 0
                                    || params.tolerance.value()
                                        != crate::params::CompTolerance::Zero
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
                                let btn_text = nih_plug_egui::egui::RichText::new(if calibrating {
                                    "CALIBRATING..."
                                } else {
                                    "CALIBRATE"
                                })
                                .font(FontId::proportional(8.5))
                                .strong()
                                .color(if calibrating {
                                    theme::TEXT_ON_ACCENT
                                } else if needs_rebuild {
                                    theme::TEXT_PRIMARY
                                } else {
                                    theme::TEXT_MUTED
                                });

                                ui.add_space(4.0);

                                let cal_btn = nih_plug_egui::egui::Button::new(btn_text)
                                    .fill(btn_color)
                                    .stroke(Stroke::new(
                                        1.0_f32,
                                        if needs_rebuild {
                                            theme::ACCENT_ORANGE
                                        } else {
                                            theme::BORDER_DEFAULT
                                        },
                                    ))
                                    .min_size(vec2(120.0, 24.0))
                                    .corner_radius(CornerRadius::same(2));

                                if ui
                                    .add(cal_btn)
                                    .on_hover_text(
                                        "Rebuild analog model with current component tolerances",
                                    )
                                    .clicked()
                                {
                                    setter.begin_set_parameter(&params.calibrate_sync);
                                    setter.set_parameter(&params.calibrate_sync, true);
                                    setter.end_set_parameter(&params.calibrate_sync);
                                }

                                ui.add_space(4.0);
                                ui.label(
                                    nih_plug_egui::egui::RichText::new(if needs_rebuild {
                                        "SYNC PENDING"
                                    } else {
                                        "CALIBRATED"
                                    })
                                    .font(FontId::proportional(7.5))
                                    .color(if needs_rebuild {
                                        theme::ACCENT_ORANGE
                                    } else {
                                        theme::TEXT_MUTED
                                    }),
                                );
                            },
                        );
                    });
                });
            });
        });
}

#[cfg(test)]
mod tests {
    #[test]
    fn tweaks_panel_column_geometry() {
        let avail_w = 1366.0 - 20.0 - 12.0; // 1334.0
        let sep_overhead = 3.0 * (6.0 + 2.0 * 8.0); // 66.0
        let col_w = (avail_w - sep_overhead) / 4.0;
        assert_eq!(col_w, 317.0);
        assert!(
            col_w > 250.0,
            "Each of the 4 tweak groups has adequate horizontal space"
        );
    }
}
