//! Offscreen renderer for the editor.
//!
//! Draws the real `render_editor` through `egui_kittest` on wgpu and writes one
//! PNG per `VIBE303_QA_STATE`, named like `scripts/capture_standalone_gui.sh`
//! names its window captures, so `crop_gui_sections.py` and `gui_qa_measure.py`
//! read the output unchanged. No window and no macOS Screen Recording
//! permission are involved.
//!
//! The frame is the editor only: no title bar, and wgpu draws it, not the GL
//! backend of the plugin window. Use it for layout, colour and state checks.
//!
//! ```text
//! cargo run --release -p gui_render -- [--out DIR] [STATE...]
//! ```

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicU8;

use egui_kittest::Harness;
use nih_plug::prelude::{GuiContext, ParamPtr, ParamSetter, PluginApi, PluginState};
use nih_plug_egui::egui::{vec2, CentralPanel, Frame, Margin};
use plugin303::gui::{qa_generator_params, render_editor, theme, GUI_HEIGHT, GUI_WIDTH, MARGIN};
use plugin303::params::Plugin303Params;
use plugin303::qa_playhead_pin;

/// `(VIBE303_QA_STATE, file stem)`. The stems are the ones the capture script
/// writes: the default state is `latest`.
const STATES: [(&str, &str); 10] = [
    ("default", "latest"),
    ("accurate", "accurate"),
    ("advanced", "advanced"),
    ("running", "running"),
    ("running_note", "running_note"),
    ("knobs", "knobs"),
    ("knob_extremes", "knob_extremes"),
    ("clipper_off", "clipper_off"),
    ("quality_low", "quality_low"),
    ("generator", "generator"),
];

/// Retina window captures are 2x, so the renders are too.
const PIXELS_PER_POINT: f32 = 2.0;

/// The editor never asks its host for state or parameter writes while drawing.
struct NoHost;

impl GuiContext for NoHost {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Standalone
    }
    fn request_resize(&self) -> bool {
        false
    }
    unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {}
    unsafe fn raw_set_parameter_normalized(&self, _param: ParamPtr, _normalized: f32) {}
    unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {}
    fn get_state(&self) -> PluginState {
        unreachable!("rendering does not query plugin state")
    }
    fn set_state(&self, _state: PluginState) {}
}

fn render(state: &str) -> Result<image::RgbaImage, String> {
    // `Plugin303Params::default()` reads the state from the environment.
    std::env::set_var("VIBE303_QA_STATE", state);
    let params = Plugin303Params::default();
    let mut gen_params = qa_generator_params(Some(state));
    let playhead = AtomicU8::new(qa_playhead_pin(Some(state)).unwrap_or(0));
    let host = NoHost;
    let setter = ParamSetter::new(&host);

    let mut first_frame = true;
    let mut harness = Harness::builder()
        .with_size(vec2(GUI_WIDTH as f32, GUI_HEIGHT as f32))
        .with_pixels_per_point(PIXELS_PER_POINT)
        .wgpu()
        .build(|ctx| {
            if std::mem::take(&mut first_frame) {
                theme::apply(ctx);
            }
            CentralPanel::default()
                .frame(
                    Frame::new()
                        .fill(theme::CHASSIS_BG)
                        .inner_margin(Margin::same(MARGIN as i8)),
                )
                .show(ctx, |ui| {
                    render_editor(ui, &params, &setter, &mut gen_params, &playhead);
                });
        });
    // Frame 1 applies the theme, the next ones lay out and settle.
    harness.run_steps(4);
    let image = harness.render()?;

    let expected = [
        (GUI_WIDTH as f32 * PIXELS_PER_POINT).round() as u32,
        (GUI_HEIGHT as f32 * PIXELS_PER_POINT).round() as u32,
    ];
    if [image.width(), image.height()] != expected {
        return Err(format!(
            "rendered {}x{}, expected {}x{}",
            image.width(),
            image.height(),
            expected[0],
            expected[1]
        ));
    }
    Ok(image)
}

fn usage() -> String {
    let names: Vec<&str> = STATES.iter().map(|(state, _)| *state).collect();
    format!(
        "usage: gui_render [--out DIR] [STATE...]\nstates: {}\ndefault: all states into target/gui-qa-offscreen",
        names.join(", ")
    )
}

fn main() -> ExitCode {
    let mut out = PathBuf::from("target/gui-qa-offscreen");
    let mut wanted: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => match args.next() {
                Some(dir) => out = PathBuf::from(dir),
                None => {
                    eprintln!("--out needs a directory\n{}", usage());
                    return ExitCode::FAILURE;
                }
            },
            "-h" | "--help" => {
                println!("{}", usage());
                return ExitCode::SUCCESS;
            }
            state => wanted.push(state.to_owned()),
        }
    }

    let selected: Vec<(&str, &str)> = if wanted.is_empty() {
        STATES.to_vec()
    } else {
        let mut selected = Vec::new();
        for name in &wanted {
            match STATES.iter().find(|(state, _)| state == name) {
                Some(entry) => selected.push(*entry),
                None => {
                    eprintln!("unknown state {name:?}\n{}", usage());
                    return ExitCode::FAILURE;
                }
            }
        }
        selected
    };

    if let Err(err) = std::fs::create_dir_all(&out) {
        eprintln!("cannot create {}: {err}", out.display());
        return ExitCode::FAILURE;
    }
    for (state, stem) in selected {
        let path = out.join(format!("{stem}.png"));
        let saved = render(state).and_then(|image| {
            image.save(&path).map_err(|err| err.to_string())?;
            Ok((image.width(), image.height()))
        });
        match saved {
            Ok((w, h)) => println!("{state}: {w}x{h} -> {}", path.display()),
            Err(err) => {
                eprintln!("{state}: {err}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
