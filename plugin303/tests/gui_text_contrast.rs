#![cfg(feature = "gui")]

mod gui_common;

use gui_common::{DummyGuiContext, EnvGuard, Rig, ENV_LOCK, TEXT_TOKENS};
use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::vec2;
use plugin303::gui::{qa_generator_params, render_editor, theme, GUI_HEIGHT, GUI_WIDTH};
use plugin303::params::Plugin303Params;
use std::sync::atomic::AtomicU8;

/// Every text the editor paints uses a text token at full strength. egui's
/// disabled fade blends a built-in widget's colour halfway to PANEL_BG, which
/// drops TEXT_SECONDARY labels to 2.9:1 on screen (round-20 #1). A colour
/// outside the token set is that fade or a hard-coded colour.
#[test]
fn every_text_uses_a_token_in_every_mode() {
    let _lock = ENV_LOCK.lock().unwrap();
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
