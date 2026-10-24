#![cfg(feature = "gui")]

//! Round-21 #8 moved NEW SEED from the ACID EUCLIDEAN GENERATOR header into
//! GENERATION, beside the SEED it changes. A click must still step the seed by
//! its own Weyl constant and change nothing else, and SEED must show the new
//! value. Driven through the real `render_generation_panel`.

mod gui_common;

use gui_common::{text_runs, DummyGuiContext, EnvGuard, Rig, ENV_LOCK};
use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{vec2, Ui};
use plugin303::dsp::generator::GeneratorParams;
use plugin303::gui::panels::generator_panel::{self, GENERATOR_H};
use plugin303::gui::{split_columns, GUI_WIDTH, MARGIN};
use plugin303::params::Plugin303Params;

/// NEW SEED's step: a Weyl sequence distinct from MUTATE's.
const NEW_SEED_STEP: u64 = 0x9E37_79B9_7F4A_7C15;

#[test]
fn new_seed_steps_only_the_seed_and_seed_shows_it() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvGuard::set("VIBE303_QA_STATE", "default");
    let params = Plugin303Params::default();
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);
    let mut gen_params = GeneratorParams::default();
    let before = gen_params.clone();
    let width = split_columns(GUI_WIDTH as f32 - 2.0 * MARGIN).1;
    let mut rig = Rig::new(vec2(width, GENERATOR_H));

    let after_click = {
        let mut add = |ui: &mut Ui| {
            generator_panel::render_generation_panel(ui, &params, &mut gen_params, &setter)
        };
        let texts = text_runs(&rig.shapes(&mut add));
        let (_, label) = texts
            .iter()
            .find(|(t, _)| t == "NEW SEED")
            .unwrap_or_else(|| panic!("GENERATION paints no NEW SEED; on screen: {texts:?}"));
        rig.click(label.center(), &mut add);
        text_runs(&rig.shapes(&mut add))
    };

    let seed = before.seed.wrapping_add(NEW_SEED_STEP);
    assert_eq!(gen_params, GeneratorParams { seed, ..before });
    let shown = format!("{:04}", seed % 10_000);
    assert!(
        after_click.iter().any(|(t, _)| *t == shown),
        "SEED does not show {shown:?} after NEW SEED; on screen: {after_click:?}"
    );
}
