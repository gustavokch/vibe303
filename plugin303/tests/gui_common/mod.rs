//! Shared rig for editor tests: drive egui frames, read painted text and its
//! colour, hover a widget. Each test binary uses a subset.
#![allow(dead_code)]

use nih_plug::prelude::{GuiContext, ParamPtr, PluginApi, PluginState};
use nih_plug_egui::egui::{
    self, epaint::Shape, vec2, CentralPanel, Color32, Context, Frame, Pos2, RawInput, Rect, Ui,
    Vec2,
};
use plugin303::gui::theme;
use std::sync::Mutex;

/// Text colours the contract allows (Colour roles). Anything else is a faded
/// or hard-coded colour.
pub const TEXT_TOKENS: [Color32; 5] = [
    theme::TEXT_PRIMARY,
    theme::TEXT_SECONDARY,
    theme::TEXT_MUTED,
    theme::TEXT_ON_ACCENT,
    theme::ACCENT_ORANGE,
];

/// Serialises `VIBE303_QA_STATE` edits: the tests of one binary share a process.
pub static ENV_LOCK: Mutex<()> = Mutex::new(());

pub struct EnvGuard<'a> {
    key: &'a str,
}

impl<'a> EnvGuard<'a> {
    pub fn set(key: &'a str, val: &str) -> Self {
        unsafe { std::env::set_var(key, val) };
        Self { key }
    }
}

impl Drop for EnvGuard<'_> {
    fn drop(&mut self) {
        unsafe { std::env::remove_var(self.key) };
    }
}

/// The editor never asks its host for state while drawing.
pub struct DummyGuiContext;

impl GuiContext for DummyGuiContext {
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
        unimplemented!("state query not exercised by editor tests")
    }
    fn set_state(&self, _state: PluginState) {}
}

/// Every text run in `shape` with the colour it is painted in. egui's fade
/// rewrites the glyph vertex colours, so this is the colour on screen.
fn collect_texts(shape: &Shape, out: &mut Vec<(String, Color32)>) {
    match shape {
        Shape::Text(t) => {
            let color = t
                .override_text_color
                .or_else(|| {
                    t.galley
                        .rows
                        .iter()
                        .find_map(|r| r.visuals.mesh.vertices.first())
                        .map(|v| v.color)
                        .filter(|c| *c != Color32::PLACEHOLDER)
                })
                .unwrap_or(t.fallback_color);
            out.push((t.galley.text().to_string(), color));
        }
        Shape::Vec(v) => v.iter().for_each(|s| collect_texts(s, out)),
        _ => {}
    }
}

pub struct Rig {
    pub ctx: Context,
    size: Vec2,
    time: f64,
}

impl Rig {
    pub fn new(size: Vec2) -> Self {
        let ctx = Context::default();
        theme::apply(&ctx);
        Self {
            ctx,
            size,
            time: 0.0,
        }
    }

    /// Run `frames` frames, 0.5 s apart, with the pointer at `pointer` (or gone).
    /// Returns the text runs of the last frame.
    pub fn run(
        &mut self,
        frames: usize,
        pointer: Option<Pos2>,
        add: &mut dyn FnMut(&mut Ui),
    ) -> Vec<(String, Color32)> {
        let mut found = Vec::new();
        for _ in 0..frames {
            self.time += 0.5;
            let mut input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
                time: Some(self.time),
                ..Default::default()
            };
            input.events.push(match pointer {
                Some(p) => egui::Event::PointerMoved(p),
                None => egui::Event::PointerGone,
            });
            let out = self.ctx.run(input, |ctx| {
                CentralPanel::default()
                    .frame(Frame::new())
                    .show(ctx, |ui| add(ui));
            });
            found.clear();
            out.shapes
                .iter()
                .for_each(|c| collect_texts(&c.shape, &mut found));
        }
        found
    }

    /// Widgets laid out in the last frame.
    pub fn widgets(&self) -> Vec<egui::WidgetRect> {
        self.ctx.viewport(|vp| {
            vp.prev_pass
                .widgets
                .layers()
                .flat_map(|(_, ws)| ws.iter().cloned())
                .collect()
        })
    }

    /// Hover the widget at `rect` past egui's 0.5 s tooltip delay; return the
    /// texts on screen. Moves the pointer away afterwards so the tooltip closes.
    pub fn hover(&mut self, rect: Rect, add: &mut dyn FnMut(&mut Ui)) -> Vec<String> {
        let at = rect.lerp_inside(vec2(0.3, 0.5));
        let texts = self.run(4, Some(at), add);
        self.run(2, None, add);
        texts.into_iter().map(|(t, _)| t).collect()
    }
}
