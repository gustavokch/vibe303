use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{
    vec2, Align, ComboBox, CornerRadius, DragValue, FontId, Frame, Layout, Margin, Stroke, Ui,
};

use crate::gui::theme;
use crate::gui::widgets::Knob303;
use crate::params::{CircuitTopology, CompTolerance, Plugin303Params};

/// Horizontal geometry of the 4-column tweaks row (round-11 item 10,
/// re-cut for round-19 issue 4).
///
/// Columns are placed on absolute rects instead of flowing through the parent
/// cursor: egui re-advances the horizontal cursor with each column's *actual*
/// content height, which tilted every following column a few px lower. With
/// fixed rects all four columns share one top edge.
///
/// The four columns are now **equal width**, cut from the panel's content
/// width, with the separators sitting exactly on the column boundaries. They
/// used to be content-hugging (232/64/108/140 logical) separated by equal
/// gaps, which put the dividers at unequal spacings — the `panels` probe read
/// the column run as 735/370/460/1126 physical px and the widest void as an
/// 870 px band. Equal columns with centred content split each column's
/// leftover in half on either side, so no single void can exceed half a
/// column.
const NUM_COLS: usize = 4;
const SEPARATOR_W: f32 = 1.0;
/// Widest content in any column, in logical px (the TOL/AGE/NOISE cluster).
/// Nothing sizes off this any more; it is the floor a column may not go below
/// without clipping, and the test below holds the layout to it.
const WIDEST_COL_CONTENT_W: f32 = 232.0;
/// One row height: seed row (20) + 2 gap + knob (60) = 82.
const ROW_H: f32 = 82.0;

pub fn render_tweaks_panel(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let is_accurate = params.topology.value() == CircuitTopology::Accurate;
    // One dimming mechanism only (round-14 issue 3): `add_enabled_ui` below
    // already fades the whole panel toward the background when the souped
    // engine is bypassed. Pre-selecting a muted token here dimmed twice and
    // landed at 2.32:1 — under the 3:1 floor. TEXT_SECONDARY fades to ≈3.5:1.
    let label_color = theme::TEXT_SECONDARY;

    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(theme::border_stroke())
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::same(6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(95.0);
            ui.vertical(|ui| {
                // Title row, outside `add_enabled_ui` on purpose. It is panel
                // chrome rather than a control, and the bypass reason beside
                // it has to stay fully legible (>= 4.5:1) while the controls
                // below it dim.
                ui.horizontal(|ui| {
                    ui.label(
                        nih_plug_egui::egui::RichText::new("ANALOG CHARACTER & TWEAKS")
                            .font(FontId::proportional(10.0))
                            .strong()
                            .color(if is_accurate {
                                // Faded ACCENT_ORANGE measures ≈2.5:1; the
                                // secondary token survives the fade at ≈3.5:1
                                // (round-14 issue 3).
                                theme::TEXT_SECONDARY
                            } else {
                                theme::ACCENT_ORANGE
                            }),
                    );

                    // Round-19 issue 5: ACCURATE bypasses this panel and used
                    // to say so nowhere — it only dimmed, while the souped
                    // panel got a full banner. The reason shares the title row
                    // so the panel's height does not change, and it lands in
                    // horizontal space the row was wasting anyway.
                    if is_accurate {
                        ui.add_space(10.0);
                        ui.label(
                            nih_plug_egui::egui::RichText::new(
                                "Analog character bypassed in ACCURATE mode",
                            )
                            .font(FontId::proportional(8.5))
                            .color(theme::TEXT_SECONDARY),
                        );
                    }
                });

                ui.add_enabled_ui(!is_accurate, |ui| {
                    // 4 equal columns on one absolute baseline.
                    let total_w = ui.available_width();
                    let sep_run_w = (NUM_COLS as f32 - 1.0) * SEPARATOR_W;
                    let col_w = (total_w - sep_run_w) / NUM_COLS as f32;
                    debug_assert!(
                        col_w >= WIDEST_COL_CONTENT_W,
                        "tweaks column ({col_w}px) is narrower than its widest \
                         content ({WIDEST_COL_CONTENT_W}px); the TOL/AGE/NOISE \
                         cluster will clip"
                    );
                    let row_top = ui.cursor().top();
                    let painter = ui.painter().clone();
                    let left = ui.cursor().left();

                    for col in 0..NUM_COLS {
                        let x = left + col as f32 * (col_w + SEPARATOR_W);
                        let col_rect = nih_plug_egui::egui::Rect::from_min_size(
                            nih_plug_egui::egui::pos2(x, row_top),
                            nih_plug_egui::egui::vec2(col_w, ROW_H),
                        );
                        if col > 0 {
                            // Painted divider (not a widget): zero layout side
                            // effects, spans the full row height. It sits on
                            // the column boundary, so the probe reads four
                            // equal columns.
                            let mid = x - SEPARATOR_W * 0.5;
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
                                                    // Caption class (8.5 pt),
                                                    // like the labels beside it
                                                    // (round-14 issue 5).
                                                    .selected_text(
                                                        nih_plug_egui::egui::RichText::new(
                                                            match tol {
                                                                CompTolerance::Zero => "0%",
                                                                CompTolerance::PointFive => "0.5%",
                                                                CompTolerance::Two => "2%",
                                                                CompTolerance::Five => "5%",
                                                                CompTolerance::Ten => "10%",
                                                            },
                                                        )
                                                        .font(FontId::proportional(8.5)),
                                                    )
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
                                                                .selectable_label(
                                                                    tol == variant,
                                                                    nih_plug_egui::egui::RichText::new(label)
                                                                        .font(FontId::proportional(8.5)),
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
                                        // Column 2: Drift knob, top-aligned
                                        1 => {
                                            ui.add(Knob303::new(
                                                &params.comp_drift,
                                                setter,
                                                "DRIFT",
                                                theme::KNOB_RADIUS_SMALL,
                                            ));
                                        }
                                        // Column 3: Seed row + Recal knob, top-aligned
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

                                            // Round-19 issue 4 retires the
                                            // shared knob baseline of round-11
                                            // item 10. That rule pushed DRIFT
                                            // and RECAL 22 px below the row
                                            // top to line their centres up,
                                            // which is exactly the mixed
                                            // vertical alignment the audit
                                            // flagged: two columns started at
                                            // the row top and two started
                                            // below it. Every column now
                                            // top-aligns, so RECAL simply
                                            // follows its seed row.
                                            ui.add_space(2.0);

                                            ui.add(Knob303::new(
                                                &params.recalibration_scale,
                                                setter,
                                                "RECAL",
                                                theme::KNOB_RADIUS_SMALL,
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
                                                } else if is_accurate {
                                                    // Muted + egui's fade lands
                                                    // at 2.3:1 (round-14 issue 3).
                                                    theme::TEXT_SECONDARY
                                                } else {
                                                    theme::TEXT_MUTED
                                                }),
                                            );
                                        }
                                    }
                                });
                            },
                        );
                    }
                    ui.advance_cursor_after_rect(nih_plug_egui::egui::Rect::from_min_size(
                        nih_plug_egui::egui::pos2(left, row_top),
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
    fn tweaks_columns_are_equal_and_fit_their_content() {
        // CentralPanel inner margin (10) + tweaks frame inner margin (6), both sides
        let avail_w = 1366.0 - 2.0 * 10.0 - 2.0 * 6.0; // 1334.0
        let sep_run_w = (NUM_COLS as f32 - 1.0) * SEPARATOR_W;
        let col_w = (avail_w - sep_run_w) / NUM_COLS as f32;

        // Round-19 issue 4: four equal columns, and they must still be wide
        // enough for the widest cluster.
        assert!(
            col_w >= WIDEST_COL_CONTENT_W,
            "column {col_w} is narrower than its widest content {WIDEST_COL_CONTENT_W}"
        );

        // The row fits the panel exactly: four columns plus three separators.
        let total_used = col_w * NUM_COLS as f32 + sep_run_w;
        assert!((total_used - avail_w).abs() <= 0.5);
    }

    #[test]
    fn tweaks_dividers_sit_on_the_column_boundaries() {
        // The `panels` probe reads column widths as the distance between
        // dividers, so equal columns are only equal if the dividers land on
        // the boundaries. Walk the same arithmetic the renderer uses.
        let avail_w = 1366.0 - 2.0 * 10.0 - 2.0 * 6.0;
        let sep_run_w = (NUM_COLS as f32 - 1.0) * SEPARATOR_W;
        let col_w = (avail_w - sep_run_w) / NUM_COLS as f32;

        let mut edges = vec![0.0_f32];
        for col in 1..NUM_COLS {
            edges.push(col as f32 * (col_w + SEPARATOR_W) - SEPARATOR_W * 0.5);
        }
        edges.push(avail_w);

        let widths: Vec<f32> = edges.windows(2).map(|w| w[1] - w[0]).collect();
        let (min, max) = widths
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), w| (lo.min(*w), hi.max(*w)));
        // 8 px physical = 4 logical px is the round-19 acceptance band.
        assert!(
            max - min <= 4.0,
            "columns must measure equal within 4 logical px, got {widths:?}"
        );
    }

    #[test]
    fn tweaks_row_height_fits_knob() {
        // Every column top-aligns now, so the tallest column is the seed row
        // (20) + its gap (2) + the knob (60).
        assert!(
            ROW_H >= 20.0 + 2.0 + 60.0,
            "row must fit the seed row and the knob below it"
        );
    }
}
