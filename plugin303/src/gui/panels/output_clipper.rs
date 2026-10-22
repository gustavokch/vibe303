use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{vec2, Align, Layout, Ui};

use crate::gui::theme;
use crate::gui::widgets::panel::DevicePanel;
use crate::gui::widgets::toggle::ToggleButton;
use crate::gui::widgets::{BoolSwitch, Knob303, SlideSwitch};
use crate::params::Plugin303Params;

/// The clipper colours the final voice output for both Accurate and Fun engines.
pub fn render_output_clipper(ui: &mut Ui, params: &Plugin303Params, setter: &ParamSetter) {
    let bypassed = params.clipper_bypass.value();

    DevicePanel::new("OUTPUT CLIPPER", crate::gui::BANK_ROW_H)
        .dimmed(bypassed)
        .show(
            ui,
            |header_ui| {
                // Header visual order left-to-right: LINK | DIODE | OS | ON.
                // The header layout is right-to-left, so add: ON, then OS, DIODE, LINK.
                let mut on = !bypassed;
                let on_resp = header_ui
                    .add(ToggleButton::new(&mut on, "ON"))
                    .on_hover_text("Lit: the clipper is in the signal path.");
                if on_resp.changed() {
                    setter.begin_set_parameter(&params.clipper_bypass);
                    setter.set_parameter(&params.clipper_bypass, !on);
                    setter.end_set_parameter(&params.clipper_bypass);
                }

                header_ui.add_enabled_ui(!bypassed, |ui| {
                    ui.add(
                        SlideSwitch::new(&params.clipper_os, setter, "OS", &["2X", "4X"])
                            .caption_left(),
                    )
                    .on_hover_text("Clipper oversampling: 4X aliases less and costs more CPU");

                    ui.add(
                        BoolSwitch::new(&params.clipper_asym, setter, "DIODE", ["SI", "GE"])
                            .caption_left(),
                    )
                    .on_hover_text(
                        "SI: silicon, symmetric clipping. GE: germanium, asymmetric clipping.",
                    );

                    let mut link = params.clipper_gain_link.value();
                    let link_resp = ui
                        .add(ToggleButton::new(&mut link, "LINK"))
                        .on_hover_text("Link: OUT tracks −IN while dragging (edit-time only)");
                    if link_resp.changed() {
                        setter.begin_set_parameter(&params.clipper_gain_link);
                        setter.set_parameter(&params.clipper_gain_link, link);
                        setter.end_set_parameter(&params.clipper_gain_link);
                        if link {
                            setter.begin_set_parameter(&params.clipper_out_gain);
                            setter.set_parameter(
                                &params.clipper_out_gain,
                                -params.clipper_in_gain.value(),
                            );
                            setter.end_set_parameter(&params.clipper_out_gain);
                        }
                    }
                });
            },
            |body_ui| {
                body_ui.add_enabled_ui(!bypassed, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                        let total_w = ui.available_width();
                        let col_w = total_w / 9.0;
                        let col_h = ui.available_height();

                        let mut render_knob = |knob: Knob303<'_, _>| {
                            ui.allocate_ui_with_layout(
                                vec2(col_w, col_h),
                                Layout::top_down(Align::Center),
                                |ui| ui.add(knob),
                            )
                            .inner
                        };

                        render_knob(
                            Knob303::new(
                                &params.clipper_drive,
                                setter,
                                "DRIVE",
                                theme::KNOB_RADIUS_MEDIUM,
                            )
                            .tooltip("Drive into the clipper"),
                        );
                        render_knob(
                            Knob303::new(
                                &params.clipper_threshold,
                                setter,
                                "THRESH",
                                theme::KNOB_RADIUS_MEDIUM,
                            )
                            .tooltip("Clipping threshold"),
                        );
                        render_knob(
                            Knob303::new(
                                &params.clipper_bass,
                                setter,
                                "BASS",
                                theme::KNOB_RADIUS_MEDIUM,
                            )
                            .tooltip("Bass into the clipper"),
                        );
                        render_knob(
                            Knob303::new(
                                &params.clipper_tone,
                                setter,
                                "TONE",
                                theme::KNOB_RADIUS_MEDIUM,
                            )
                            .tooltip("Clipper tone"),
                        );
                        let in_resp = render_knob(
                            Knob303::new(
                                &params.clipper_in_gain,
                                setter,
                                "IN",
                                theme::KNOB_RADIUS_MEDIUM,
                            )
                            .tooltip("Gain into the clipper (dB)"),
                        );
                        let out_resp = render_knob(
                            Knob303::new(
                                &params.clipper_out_gain,
                                setter,
                                "OUT",
                                theme::KNOB_RADIUS_MEDIUM,
                            )
                            .tooltip("Gain after the clipper (dB)"),
                        );
                        render_knob(
                            Knob303::new(
                                &params.clipper_tilt,
                                setter,
                                "TILT",
                                theme::KNOB_RADIUS_MEDIUM,
                            )
                            .tooltip("Spectral tilt after the clipper (dB)"),
                        );
                        render_knob(
                            Knob303::new(
                                &params.clipper_dynamics,
                                setter,
                                "DYN",
                                theme::KNOB_RADIUS_MEDIUM,
                            )
                            .tooltip(
                                "Dynamics: loud passages duck the drive, so attacks pass through",
                            ),
                        );
                        render_knob(
                            Knob303::new(
                                &params.clipper_mix,
                                setter,
                                "MIX",
                                theme::KNOB_RADIUS_MEDIUM,
                            )
                            .tooltip("Dry/wet mix of the clipper"),
                        );

                        // Gain link: mirror only on genuine user gestures (drag, double-click reset)
                        if params.clipper_gain_link.value() {
                            if in_resp.drag_started() || in_resp.double_clicked() {
                                setter.begin_set_parameter(&params.clipper_out_gain);
                            }
                            if in_resp.dragged() || in_resp.double_clicked() {
                                setter.set_parameter(
                                    &params.clipper_out_gain,
                                    -params.clipper_in_gain.value(),
                                );
                            }
                            if in_resp.drag_stopped() || in_resp.double_clicked() {
                                setter.end_set_parameter(&params.clipper_out_gain);
                            }
                            if out_resp.drag_started() || out_resp.double_clicked() {
                                setter.begin_set_parameter(&params.clipper_in_gain);
                            }
                            if out_resp.dragged() || out_resp.double_clicked() {
                                setter.set_parameter(
                                    &params.clipper_in_gain,
                                    -params.clipper_out_gain.value(),
                                );
                            }
                            if out_resp.drag_stopped() || out_resp.double_clicked() {
                                setter.end_set_parameter(&params.clipper_in_gain);
                            }
                        }
                    });
                });
            },
        );
}
