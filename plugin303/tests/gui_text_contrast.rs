#![cfg(feature = "gui")]

mod gui_common;

use gui_common::{filled_rects, text_runs, DummyGuiContext, EnvGuard, Rig, ENV_LOCK, TEXT_TOKENS};
use nih_plug::prelude::{Param, ParamSetter};
use nih_plug_egui::egui::{pos2, vec2, Color32, Context, FontId, Rect, Shape, Ui};
use plugin303::gui::panels::sequencer_panel::{render_sequencer_grid, SEQUENCER_H};
use plugin303::gui::panels::tweaks_panel;
use plugin303::gui::widgets::piano_roll_popup::{set_open_state, OpenPianoRollState};
use plugin303::gui::{
    qa_generator_params, render_editor, split_columns, theme, GUI_HEIGHT, GUI_WIDTH, MARGIN,
    TWEAKS_H,
};
use plugin303::params::Plugin303Params;
use std::sync::atomic::AtomicU8;

/// Every text the editor paints uses a text token at full strength. egui's
/// disabled fade blends a built-in widget's colour halfway to PANEL_BG, which
/// drops TEXT_SECONDARY labels to 2.9:1 on screen (round-20 #1). A colour
/// outside the token set is that fade or a hard-coded colour.
#[test]
fn every_text_uses_a_token_in_every_mode() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for state in ["default", "accurate", "advanced", "running", "len12"] {
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

/// The colour `text` is painted in.
fn colour_of(texts: &[(String, Color32)], text: &str) -> Color32 {
    texts
        .iter()
        .find(|(t, _)| t == text)
        .map(|(_, c)| *c)
        .unwrap_or_else(|| panic!("{text:?} is not painted; on screen: {texts:?}"))
}

/// A step past LEN never plays. Its number is muted and its flags show no
/// hue; the last step of the loop keeps both. The whole grid looked live at
/// any LEN (round-23 #3).
#[test]
fn steps_past_len_are_muted_and_carry_no_hue() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvGuard::set("VIBE303_QA_STATE", "len12");
    let params = Plugin303Params::default();
    {
        let mut pattern = params.pattern.write().unwrap();
        pattern.steps[11].accent = true; // step 12, the last of the loop
        pattern.steps[12].accent = true; // step 13
        pattern.steps[13].slide = true; // step 14
    }
    let step = AtomicU8::new(0);
    let left = split_columns(GUI_WIDTH as f32 - 2.0 * MARGIN).0;
    let mut rig = Rig::new(vec2(left, SEQUENCER_H));
    let mut add = |ui: &mut Ui| render_sequencer_grid(ui, &params, &step);

    let texts = rig.run(2, None, &mut add);
    assert_eq!(colour_of(&texts, "12"), theme::TEXT_SECONDARY, "step 12");
    for past in ["13", "14", "15", "16"] {
        assert_eq!(colour_of(&texts, past), theme::TEXT_MUTED, "step {past}");
    }

    let shapes = rig.shapes(&mut add);
    let numbers = text_runs(&shapes);
    // A hue-filled pill in the flag row under the number of step `s`.
    let hue_under = |s: &str| {
        let number = numbers
            .iter()
            .find(|(t, _)| t == s)
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("step {s} is not painted"));
        [theme::ACCENT_ORANGE, theme::ACCENT_BLUE]
            .into_iter()
            .flat_map(|hue| filled_rects(&shapes, hue))
            .any(|pill| {
                (pill.center().x - number.center().x).abs() < 45.0
                    && (0.0..40.0).contains(&(pill.top() - number.top()))
            })
    };
    assert!(hue_under("12"), "step 12 is in the loop: its ACC stays lit");
    for past in ["13", "14"] {
        assert!(!hue_under(past), "step {past} is past LEN and shows a hue");
    }
}

/// NOISE prints a temperature above its lowest setting. The degree sign has
/// a glyph in the font of the field and of its ACCURATE readout.
#[test]
fn noise_readout_has_its_glyphs() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvGuard::set("VIBE303_QA_STATE", "default");
    let warmest = Plugin303Params::default()
        .thermal_noise
        .normalized_value_to_string(1.0, true);
    let mut rig = Rig::new(vec2(100.0, 100.0));
    rig.run(1, None, &mut |_ui| {});
    assert!(
        rig.ctx
            .fonts(|f| f.has_glyphs(&FontId::monospace(9.0), &warmest)),
        "{warmest:?} has a character the monospace face lacks"
    );
}

/// A disabled card holds no `TEXT_PRIMARY`: a knob's value reads like the
/// Readout beside it. The knob values were the brightest text in ACCURATE's
/// ANALOG TWEAKS (round-23 #6).
#[test]
fn disabled_values_share_one_weight() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvGuard::set("VIBE303_QA_STATE", "accurate");
    let params = Plugin303Params::default();
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);
    let right = split_columns(GUI_WIDTH as f32 - 2.0 * MARGIN).1;
    let texts = Rig::new(vec2(right, TWEAKS_H)).run(2, None, &mut |ui| {
        tweaks_panel::render_tweaks_panel(ui, &params, &setter)
    });
    let bright: Vec<&String> = texts
        .iter()
        .filter(|(_, c)| *c == theme::TEXT_PRIMARY)
        .map(|(t, _)| t)
        .collect();
    assert!(
        bright.is_empty(),
        "TEXT_PRIMARY in a disabled card: {bright:?}"
    );
}
