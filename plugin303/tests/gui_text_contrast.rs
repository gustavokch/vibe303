#![cfg(feature = "gui")]

mod gui_common;

use gui_common::{DummyGuiContext, EnvGuard, Rig, ENV_LOCK, TEXT_TOKENS};
use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{pos2, vec2, Context, Rect, Shape};
use plugin303::gui::widgets::piano_roll_popup::{set_open_state, OpenPianoRollState};
use plugin303::gui::{qa_generator_params, render_editor, theme, GUI_HEIGHT, GUI_WIDTH};
use plugin303::params::Plugin303Params;
use std::sync::atomic::AtomicU8;

/// Every text the editor paints uses a text token at full strength. egui's
/// disabled fade blends a built-in widget's colour halfway to PANEL_BG, which
/// drops TEXT_SECONDARY labels to 2.9:1 on screen (round-20 #1). A colour
/// outside the token set is that fade or a hard-coded colour.
#[test]
fn every_text_uses_a_token_in_every_mode() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for state in ["default", "accurate", "advanced", "running"] {
        let _env = EnvGuard::set("VIBE303_QA_STATE", state);
        let params = Plugin303Params::default();
        let host = DummyGuiContext;
        let setter = ParamSetter::new(&host);
        let mut gen_params = qa_generator_params(Some(state));
        let step = AtomicU8::new(3);
        let mut rig = Rig::new(vec2(GUI_WIDTH as f32, GUI_HEIGHT as f32));
        let texts = rig.run(2, None, &mut |ui| {
            render_editor(ui, &params, &setter, &mut gen_params, &step)
        });
        assert!(
            texts.len() > 100,
            "{state}: editor painted only {} texts",
            texts.len()
        );
        let off: Vec<String> = texts
            .iter()
            .filter(|(_, c)| !TEXT_TOKENS.contains(c))
            .map(|(t, c)| {
                format!(
                    "{t:?} {c:?} = {:.2}:1 on PANEL_BG",
                    theme::contrast_ratio(*c, theme::PANEL_BG)
                )
            })
            .collect();
        assert!(
            off.is_empty(),
            "{state}: text outside the token set: {off:#?}"
        );
    }
}

/// The text runs in `shapes` with a character their font has no glyph for.
fn missing_glyphs(ctx: &Context, shapes: &[Shape]) -> Vec<String> {
    shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Text(t) => Some(&t.galley.job),
            _ => None,
        })
        .flat_map(|job| {
            job.sections.iter().filter_map(|section| {
                let text = &job.text[section.byte_range.clone()];
                let shown = text.replace('\n', "");
                let ok = ctx.fonts(|f| f.has_glyphs(&section.format.font_id, &shown));
                (!ok).then(|| text.to_string())
            })
        })
        .collect()
}

/// Every text the editor paints, the open piano roll included, has a glyph in
/// its font for each character. A missing glyph paints an empty box: the piano
/// roll's close mark "✕" did (round-22 #1).
#[test]
fn every_text_has_its_glyphs() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for state in ["default", "accurate"] {
        let _env = EnvGuard::set("VIBE303_QA_STATE", state);
        let params = Plugin303Params::default();
        let host = DummyGuiContext;
        let setter = ParamSetter::new(&host);
        let mut gen_params = qa_generator_params(Some(state));
        let step = AtomicU8::new(0);
        let mut rig = Rig::new(vec2(GUI_WIDTH as f32, GUI_HEIGHT as f32));
        let mut add = |ui: &mut nih_plug_egui::egui::Ui| {
            render_editor(ui, &params, &setter, &mut gen_params, &step)
        };
        rig.run(2, None, &mut add);
        set_open_state(
            &rig.ctx,
            Some(OpenPianoRollState {
                step_idx: 0,
                anchor_rect: Rect::from_min_size(pos2(16.0, 490.0), vec2(60.0, 19.0)),
            }),
        );
        let shapes = rig.shapes(&mut add);
        assert!(
            gui_common::text_runs(&shapes)
                .iter()
                .any(|(t, _)| t.starts_with("STEP 01")),
            "{state}: the piano roll did not open"
        );
        let missing = missing_glyphs(&rig.ctx, &shapes);
        assert!(
            missing.is_empty(),
            "{state}: texts with a glyph the font lacks: {missing:?}"
        );
    }
}
