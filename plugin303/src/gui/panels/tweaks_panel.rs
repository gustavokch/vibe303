use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    vec2, Align, ComboBox, CornerRadius, DragValue, FontId, Frame, Layout, Margin, Stroke, Ui,
};

use crate::gui::theme;
use crate::gui::widgets::Knob303;
use crate::params::{CircuitTopology, CompTolerance, Plugin303Params};

/// Horizontal geometry of the 4-column tweaks row (round-11 item 10).
///
/// Columns are placed on absolute rects instead of flowing through the parent
/// cursor: egui re-advances the horizontal cursor with each column's *actual*
/// content height, which tilted every following column a few px lower. With
/// fixed rects all four columns share one top edge, and the two knobs share a
/// baseline by construction (both start KNOB_TOP below it).
const NUM_COLS: usize = 4;
/// Content-hugging column widths; the leftover panel width is split into equal
/// side gaps so no column shows a one-sided trailing void.
const COL_W: [f32; NUM_COLS] = [232.0, 64.0, 108.0, 140.0];
const SEPARATOR_W: f32 = 1.0;
/// One row height: seed row (20) + 2 gap + knob (60) = 82.
const ROW_H: f32 = 82.0;
/// Knob columns start their knob this far below the row top, so DRIFT and
/// RECAL share a vertical centre line without a placeholder row.
const KNOB_TOP: f32 = 22.0;

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

                    // 4 columns on one absolute baseline (round-11 item 10).
                    let total_w = ui.available_width();
                    let sep_run_w = 3.0 * SEPARATOR_W;
                    let content_w: f32 = COL_W.iter().sum();
                    // 2*NUM_COLS equal gaps: one outside each outer column and
                    // one on each side of every separator, so mid-gaps read as
                    // gap + divider + gap and no column hugs a divider.
                    let gap = (total_w - content_w - sep_run_w) / ((2 * NUM_COLS) as f32);
                    let row_top = ui.cursor().top();
                    let painter = ui.painter().clone();

                    let mut x = ui.cursor().left() + gap;
                    for (col, &w) in COL_W.iter().enumerate() {
                        let col_rect = nih_plug_egui::egui::Rect::from_min_size(
                            nih_plug_egui::egui::pos2(x, row_top),
                            nih_plug_egui::egui::vec2(w, ROW_H),
                        );
                        if col > 0 {
                            // Painted divider (not a widget): zero layout side
                            // effects, spans the full row height.
                            let mid = x - gap * 0.5;
                            painter.line_segment(
                                [
                                    nih_plug_egui::egui::pos2(mid, row_top + 6.0),
                                    nih_plug_egui::egui::pos2(mid, row_top + ROW_H - 6.0),
                                ],
                                Stroke::new(1.0_f32, theme::BORDER_DEFAULT),
                            );
                        }
                        ui.allocate_new_ui(
                            nih_plug_egui::egui::UiBuilder::new().max_rect(col_rect),
                            |ui| {
                                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                                    match col {
                                        // Column 1: Component Tolerance & Aging
                                        0 => {
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
                                                            .custom_formatter(|v, _| {
                                                                format!("{:.0} yrs", v)
                                                            }),
                                                    )
                                                    .on_hover_text(
                                                        "Component Aging (0-100 years drag)",
                                                    )
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
                                                            .custom_formatter(|v, _| {
                                                                format!("{:.0}%", v)
                                                            }),
                                                    )
                                                    .on_hover_text(
                                                        "Thermal Noise floor (0-100% drag)",
                                                    )
                                                    .changed()
                                                {
                                                    setter.begin_set_parameter(
                                                        &params.thermal_noise,
                                                    );
                                                    setter.set_parameter(
                                                        &params.thermal_noise,
                                                        noise_val,
                                                    );
                                                    setter.end_set_parameter(
                                                        &params.thermal_noise,
                                                    );
                                                }
                                            });
                                        }
                                        // Column 2: Drift knob on the shared baseline
                                        1 => {
                                            ui.add_space(KNOB_TOP);
                                            ui.add(Knob303::new(
                                                &params.comp_drift,
                                                setter,
                                                "DRIFT",
                                                11.0,
                                            ));
                                        }
                                        // Column 3: Seed row + Recal knob on the same baseline
                                        2 => {
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                                                ui.label(
                                                    nih_plug_egui::egui::RichText::new("CMP SEED:")
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

                                            // Exact baseline: pad whatever the seed row
                                            // actually measured up to KNOB_TOP, so RECAL
                                            // shares DRIFT's vertical centre to the pixel
                                            // (round-11 item 10).
                                            let used = ui.cursor().top() - col_rect.top();
                                            ui.add_space(KNOB_TOP - used);

                                            ui.add(Knob303::new(
                                                &params.recalibration_scale,
                                                setter,
                                                "RECAL",
                                                11.0,
                                            ));
                                        }
                                        // Column 4: Calibration action & status
                                        _ => {
                                            let needs_rebuild = params.recalibration_scale.value()
                                                != 1.0
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
                                            let btn_text = nih_plug_egui::egui::RichText::new(
                                                if calibrating {
                                                    "CALIBRATING..."
                                                } else {
                                                    "CALIBRATE"
                                                },
                                            )
                                            .font(FontId::proportional(8.5))
                                            .strong()
                                            // Enabled-button label tier (round-11
                                            // item 10): white like every other
                                            // button; muted read as disabled.
                                            .color(if calibrating {
                                                theme::TEXT_ON_ACCENT
                                            } else {
                                                theme::TEXT_PRIMARY
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
                                                nih_plug_egui::egui::RichText::new(
                                                    if needs_rebuild {
                                                        "SYNC PENDING"
                                                    } else {
                                                        "CALIBRATED"
                                                    },
                                                )
                                                .font(FontId::proportional(7.5))
                                                .color(if needs_rebuild {
                                                    theme::ACCENT_ORANGE
                                                } else {
                                                    theme::TEXT_MUTED
                                                }),
                                            );
                                        }
                                    }
                                });
                            },
                        );
                        x += w + gap;
                        if col < NUM_COLS - 1 {
                            x += SEPARATOR_W + gap;
                        }
                    }
                    ui.advance_cursor_after_rect(nih_plug_egui::egui::Rect::from_min_size(
                        nih_plug_egui::egui::pos2(ui.cursor().left(), row_top),
                        nih_plug_egui::egui::vec2(total_w, ROW_H),
                    ));
                });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tweaks_row_fits_panel_with_positive_gaps() {
        // CentralPanel inner margin (10) + tweaks frame inner margin (6), both sides
        let avail_w = 1366.0 - 2.0 * 10.0 - 2.0 * 6.0; // 1334.0
        let content_w: f32 = COL_W.iter().sum();
        let gap = (avail_w - content_w - 3.0 * SEPARATOR_W) / (2.0 * NUM_COLS as f32);
        assert!(gap > 24.0, "the side gaps must stay real gaps, got {gap}");
        // The row must fit inside the panel: outer gap + content + gaps + separators.
        let total_used = content_w + 3.0 * SEPARATOR_W + 2.0 * NUM_COLS as f32 * gap;
        assert!(total_used <= avail_w + 0.5);
    }

    #[test]
    fn tweaks_knob_columns_share_baseline() {
        // Both knob columns place their knob KNOB_TOP below the row top; the
        // seed row (20) + spacing (2) above RECAL adds up to the same offset.
        assert_eq!(20.0 + 2.0, KNOB_TOP);
    }

    #[test]
    fn tweaks_row_height_fits_knob() {
        assert!(
            ROW_H >= KNOB_TOP + 60.0,
            "row must fit knob below baseline offset"
        );
    }
}
