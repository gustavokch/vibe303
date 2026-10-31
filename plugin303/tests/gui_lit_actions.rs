#![cfg(feature = "gui")]

//! Every action answers the pointer. A lit action (CONFIRM CLEAR, CALIBRATE
//! when a rebuild is due, ENABLE FUN MODE) lifts from `ACCENT_ORANGE` to
//! `ACCENT_ORANGE_HOVER`, like every lit tile and the lit ACC flag. A raised
//! action lifts from `SURFACE_ELEVATED` to `SURFACE_HOVER`. `Button::fill`
//! holds one fill in every state, so it did neither.

mod gui_common;

use gui_common::{fill_under, text_runs, DummyGuiContext, EnvGuard, Rig, ENV_LOCK};
use nih_plug::prelude::ParamSetter;
use nih_plug_egui::egui::{epaint::Shape, vec2, Color32, Rect, Ui};
use plugin303::gui::panels::sequencer_panel::{render_transport_panel, SEQUENCER_H};
use plugin303::gui::panels::{souped_panel, tweaks_panel};
use plugin303::gui::{split_columns, theme, BANK_ROW_H, GUI_WIDTH, MARGIN, TWEAKS_H};
use plugin303::params::{Plugin303Params, VcoTrim};

/// The rect of the painted label `label`.
fn label_rect(shapes: &[Shape], label: &str) -> Rect {
    let texts = text_runs(shapes);
    texts
        .iter()
        .find(|(t, _)| t == label)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("{label:?} is not painted; on screen: {texts:?}"))
}

/// The fill of the tile under `label`, with the pointer gone and then on it.
fn rest_and_hover(rig: &mut Rig, label: &str, add: &mut dyn FnMut(&mut Ui)) -> (Color32, Color32) {
    let mut fill = |rig: &mut Rig, on_label: bool| {
        let at = label_rect(&rig.shapes_at(None, add), label).center();
        let shapes = rig.shapes_at(on_label.then_some(at), add);
        fill_under(&shapes, label_rect(&shapes, label))
            .unwrap_or_else(|| panic!("{label:?} sits on no tile"))
    };
    (fill(rig, false), fill(rig, true))
}

fn columns() -> (f32, f32) {
    split_columns(GUI_WIDTH as f32 - 2.0 * MARGIN)
}

#[test]
fn lit_actions_lift_under_the_pointer() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);
    let (left, right) = columns();
    let lit = (theme::ACCENT_ORANGE, theme::ACCENT_ORANGE_HOVER);

    {
        let _env = EnvGuard::set("VIBE303_QA_STATE", "default");
        let params = Plugin303Params::default();
        let mut rig = Rig::new(vec2(right, SEQUENCER_H));
        let mut add = |ui: &mut Ui| render_transport_panel(ui, &params, &setter);
        let clear = label_rect(&rig.shapes(&mut add), "CLEAR");
        rig.click(clear.center(), &mut add); // arms it
        assert_eq!(
            rest_and_hover(&mut rig, "CONFIRM CLEAR", &mut add),
            lit,
            "CONFIRM CLEAR: (at rest, under the pointer)"
        );
    }

    {
        let _env = EnvGuard::set("VIBE303_QA_STATE", "default");
        let params = Plugin303Params::default();
        // The parameters ask for another trim than the stored one.
        *params.vco_trim.write().unwrap() = VcoTrim {
            age: 5,
            ..VcoTrim::NEUTRAL
        };
        let mut rig = Rig::new(vec2(right, TWEAKS_H));
        let mut add = |ui: &mut Ui| tweaks_panel::render_tweaks_panel(ui, &params, &setter);
        rig.shapes(&mut add);
        assert_eq!(
            rest_and_hover(&mut rig, "CALIBRATE", &mut add),
            lit,
            "CALIBRATE with a rebuild due: (at rest, under the pointer)"
        );
    }

    {
        let _env = EnvGuard::set("VIBE303_QA_STATE", "accurate");
        let params = Plugin303Params::default();
        let mut rig = Rig::new(vec2(left, BANK_ROW_H));
        let mut add = |ui: &mut Ui| souped_panel::render_souped_panel(ui, &params, &setter);
        rig.shapes(&mut add);
        assert_eq!(
            rest_and_hover(&mut rig, "ENABLE FUN MODE", &mut add),
            lit,
            "ENABLE FUN MODE: (at rest, under the pointer)"
        );
    }
}

#[test]
fn unarmed_clear_lifts_like_a_raised_action() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvGuard::set("VIBE303_QA_STATE", "default");
    let host = DummyGuiContext;
    let setter = ParamSetter::new(&host);
    let params = Plugin303Params::default();
    let mut rig = Rig::new(vec2(columns().1, SEQUENCER_H));
    let mut add = |ui: &mut Ui| render_transport_panel(ui, &params, &setter);
    rig.shapes(&mut add);
    assert_eq!(
        rest_and_hover(&mut rig, "CLEAR", &mut add),
        (theme::SURFACE_ELEVATED, theme::SURFACE_HOVER),
        "CLEAR: (at rest, under the pointer)"
    );
}
