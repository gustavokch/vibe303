//! Shared rig for editor tests: drive egui frames, read painted text and its
//! colour or its rect, hover or click a widget. Each test binary uses a subset.
#![allow(dead_code)]

use nih_plug::prelude::{GuiContext, ParamPtr, PluginApi, PluginState};
use nih_plug_egui::egui::{
    self, epaint::Shape, vec2, CentralPanel, Color32, Context, Frame, Pos2, RawInput, Rect, Ui,
    Vec2,
};
use plugin303::gui::theme;
use std::ffi::OsString;
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
/// A failed test poisons it; callers take the guard with
/// `ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())` so one failure does not
/// cascade into the rest.
pub static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Sets an environment variable for its lifetime and restores the previous
/// value on drop (removes it if it was unset).
pub struct EnvGuard<'a> {
    key: &'a str,
    prev: Option<OsString>,
}

impl<'a> EnvGuard<'a> {
    pub fn set(key: &'a str, val: &str) -> Self {
        let prev = std::env::var_os(key);
        unsafe { std::env::set_var(key, val) };
        Self { key, prev }
    }
}

impl Drop for EnvGuard<'_> {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => unsafe { std::env::set_var(self.key, v) },
            None => unsafe { std::env::remove_var(self.key) },
        }
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

/// The text run of `shape`, if it is one, with the colour it is painted in.
/// egui's fade rewrites the glyph vertex colours, so this is the colour on
/// screen.
fn collect_texts(shape: &Shape, out: &mut Vec<(String, Color32)>) {
    if let Shape::Text(t) = shape {
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
}

/// `shape`, with nested `Shape::Vec`s spread into `out`.
fn flatten(shape: Shape, out: &mut Vec<Shape>) {
    match shape {
        Shape::Vec(v) => v.into_iter().for_each(|s| flatten(s, out)),
        s => out.push(s),
    }
}

/// Every text run in `shapes` with its layout rect on screen.
pub fn text_runs(shapes: &[Shape]) -> Vec<(String, Rect)> {
    shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Text(t) => Some((
                t.galley.text().to_string(),
                t.galley.rect.translate(t.pos.to_vec2()),
            )),
            _ => None,
        })
        .collect()
}

/// The rects in `shapes` filled with `color`.
pub fn filled_rects(shapes: &[Shape], color: Color32) -> Vec<Rect> {
    shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Rect(r) if r.fill == color => Some(r.rect),
            _ => None,
        })
        .collect()
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

    /// One frame `dt` seconds after the previous one, fed `events`. Returns the
    /// shapes it painted, flattened.
    fn pass(
        &mut self,
        dt: f64,
        events: Vec<egui::Event>,
        add: &mut dyn FnMut(&mut Ui),
    ) -> Vec<Shape> {
        self.time += dt;
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let out = self.ctx.run(input, |ctx| {
            CentralPanel::default()
                .frame(Frame::new())
                .show(ctx, |ui| add(ui));
        });
        let mut shapes = Vec::new();
        out.shapes
            .into_iter()
            .for_each(|c| flatten(c.shape, &mut shapes));
        shapes
    }

    /// One frame, as `pass`. Returns the text runs it painted.
    fn frame(
        &mut self,
        dt: f64,
        events: Vec<egui::Event>,
        add: &mut dyn FnMut(&mut Ui),
    ) -> Vec<(String, Color32)> {
        let mut found = Vec::new();
        self.pass(dt, events, add)
            .iter()
            .for_each(|s| collect_texts(s, &mut found));
        found
    }

    /// Settle the layout with the pointer gone, then return the shapes of one
    /// more frame.
    pub fn shapes(&mut self, add: &mut dyn FnMut(&mut Ui)) -> Vec<Shape> {
        self.run(2, None, add);
        self.pass(0.5, vec![egui::Event::PointerGone], add)
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
            let at = match pointer {
                Some(p) => egui::Event::PointerMoved(p),
                None => egui::Event::PointerGone,
            };
            found = self.frame(0.5, vec![at], add);
        }
        found
    }

    /// Jump the frame clock to `time` seconds; the next frame lands one `dt` after it.
    pub fn set_time(&mut self, time: f64) {
        self.time = time;
    }

    /// Click the primary button at `at`: move, press, release, then one frame
    /// to settle, 10 ms apart. Returns the text runs of the last frame.
    pub fn click(&mut self, at: Pos2, add: &mut dyn FnMut(&mut Ui)) -> Vec<(String, Color32)> {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        self.frame(0.01, vec![egui::Event::PointerMoved(at)], add);
        self.frame(0.01, vec![button(true)], add);
        self.frame(0.01, vec![button(false)], add);
        self.frame(0.01, vec![], add)
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
