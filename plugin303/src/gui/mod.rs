pub mod panels;
pub mod theme;
pub mod widgets;

use nih_plug::prelude::{Editor, ParamSetter};
use nih_plug_egui::egui::{
    pos2, vec2, Align, CentralPanel, Frame, Layout, Margin, Rect, Ui, UiBuilder,
};
use nih_plug_egui::{create_egui_editor, EguiState};
use std::sync::atomic::AtomicU8;
use std::sync::Arc;

use crate::dsp::generator::GeneratorParams;
use crate::params::{Plugin303Params, QaState};

/// Window size in logical px. The aspect ratio is 19:10 (wide and short, like a
/// device chain), so `GUI_WIDTH * 10 == GUI_HEIGHT * 19`.
pub const GUI_WIDTH: u32 = 1216;
pub const GUI_HEIGHT: u32 = 640;
const _: () = assert!(GUI_WIDTH * 10 == GUI_HEIGHT * 19, "window must stay 19:10");

/// Window margin in logical px: the chassis plane showing around the devices.
pub const MARGIN: f32 = 4.0;

/// Uniform gutter between devices, horizontal and vertical. Every gap in the
/// stack is an explicit `add_space(PANEL_GAP)` or a column split of this width,
/// with the automatic `item_spacing.y` zeroed.
pub const PANEL_GAP: f32 = 3.0;

/// Height of the app bar at the top of the window.
pub const HEADER_BAR_H: f32 = 24.0;

/// Device height for a row of LARGE knobs: header strip + knob + padding.
pub const VOICE_ROW_H: f32 = 100.0;

/// Device height for a row of MEDIUM knobs.
pub const BANK_ROW_H: f32 = 80.0;

/// The tweaks device spans the two knob-bank rows beside it.
pub const TWEAKS_H: f32 = 2.0 * BANK_ROW_H + PANEL_GAP;

/// Share of the row the left column takes. Every row below the app bar splits
/// at this one seam, so device edges line up down the whole window.
const LEFT_COLUMN_FRACTION: f32 = 2.0 / 3.0;

/// Left and right column widths for a row `total` wide.
pub fn split_columns(total: f32) -> (f32, f32) {
    let left = ((total - PANEL_GAP) * LEFT_COLUMN_FRACTION).round();
    (left, total - PANEL_GAP - left)
}

/// Lay out one row as two columns on the shared seam. `add` receives the left
/// and right column Uis; each column stacks its devices top-down. The parent's
/// cursor advances past whichever column ran longer.
fn columns(ui: &mut Ui, id: &'static str, add: impl FnOnce(&mut Ui, &mut Ui)) {
    let (left_w, right_w) = split_columns(ui.available_width());
    let origin = ui.cursor().min;
    let height = ui.available_height();
    let left_rect = Rect::from_min_size(origin, vec2(left_w, height));
    let right_rect = Rect::from_min_size(
        pos2(origin.x + left_w + PANEL_GAP, origin.y),
        vec2(right_w, height),
    );
    let mut left = ui.new_child(
        UiBuilder::new()
            .id_salt((id, "left"))
            .max_rect(left_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    let mut right = ui.new_child(
        UiBuilder::new()
            .id_salt((id, "right"))
            .max_rect(right_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    add(&mut left, &mut right);
    ui.advance_cursor_after_rect(left.min_rect().union(right.min_rect()));
}

/// The whole editor: every device from the app bar down. The production editor
/// and the layout test both render through this, so they cannot diverge.
pub fn render_editor(
    ui: &mut Ui,
    params: &Plugin303Params,
    setter: &ParamSetter,
    gen_params: &mut GeneratorParams,
    current_step: &AtomicU8,
) {
    // Zero the automatic widget spacing: the gutters are explicit PANEL_GAP only.
    ui.spacing_mut().item_spacing.y = 0.0;

    panels::header_bar::render_header(ui, params, setter);
    ui.add_space(PANEL_GAP);

    columns(ui, "voice", |left, right| {
        panels::top_controls::render_voice_panel(left, params, setter);
        panels::top_controls::render_envelope_panel(right, params, setter);
    });
    ui.add_space(PANEL_GAP);

    columns(ui, "engine", |left, right| {
        panels::souped_panel::render_souped_panel(left, params, setter);
        left.add_space(PANEL_GAP);
        panels::output_clipper::render_output_clipper(left, params, setter);
        panels::tweaks_panel::render_tweaks_panel(right, params, setter);
    });
    ui.add_space(PANEL_GAP);

    columns(ui, "generator", |left, right| {
        panels::generator_panel::render_generator_panel(left, gen_params);
        panels::generator_panel::render_generation_panel(right, params, gen_params, setter);
    });
    ui.add_space(PANEL_GAP);

    columns(ui, "sequencer", |left, right| {
        panels::sequencer_panel::render_sequencer_grid(left, params, current_step);
        panels::sequencer_panel::render_transport_panel(right, params, setter);
    });
}

/// Generator preset of a `VIBE303_QA_STATE` capture: every `generator` field
/// moved off its default. Any other state gets `GeneratorParams::default()`.
/// `create_editor` and the offscreen renderer (`tools/gui_render`) both read
/// this, so a window capture and its offscreen render cannot drift.
pub fn qa_generator_params(state: Option<&str>) -> GeneratorParams {
    match state.and_then(QaState::parse) {
        Some(QaState::Generator) => GeneratorParams {
            root_note: 3, // D#
            scale: crate::dsp::generator::MusicalScale::Phrygian,
            // Narrower than GeneratorParams::default() (-1..=1) by product
            // choice: the shipped QA preset stays in 0..=1. Align to the
            // struct default only with sign-off.
            octave_min: 0,
            octave_max: 1,
            rhythm_pulses: 7,
            rhythm_length: 12,
            rhythm_shift: 3,
            pitch_length: 9,
            rest_probability: 0.15,
            accent_probability: 0.45,
            slide_probability: 0.35,
            octave_up_prob: 0.3,
            octave_down_prob: 0.2,
            accent_pitch_bias: 0.7,
            slide_chaining: true,
            mutation_rate: 0.25,
            seed: 1337,
        },
        _ => GeneratorParams::default(),
    }
}

pub fn create_editor(
    params: Arc<Plugin303Params>,
    editor_state: Arc<EguiState>,
    current_playing_step: Arc<AtomicU8>,
) -> Option<Box<dyn Editor>> {
    let qa_state = std::env::var("VIBE303_QA_STATE").ok();
    let gen_params_state = Arc::new(std::sync::Mutex::new(qa_generator_params(
        qa_state.as_deref(),
    )));

    create_egui_editor(
        editor_state,
        (),
        move |egui_ctx, _| {
            egui_ctx.send_viewport_cmd(nih_plug_egui::egui::ViewportCommand::Title(
                "TECHNEO 303".into(),
            ));
            theme::apply(egui_ctx);
            // The window cannot follow egui's zoom, so Cmd/Ctrl +/- would leave a dead band.
            egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        },
        move |egui_ctx, setter, _state| {
            CentralPanel::default()
                .frame(
                    Frame::new()
                        .fill(theme::CHASSIS_BG)
                        .inner_margin(Margin::same(MARGIN as i8)),
                )
                .show(egui_ctx, |ui| {
                    let mut gen_params = gen_params_state.lock().unwrap();
                    render_editor(ui, &params, setter, &mut gen_params, &current_playing_step);
                });
        },
    )
}
