//! A self-contained harness for driving the application in tests.
//!
//! It builds the same `Ui` eframe builds, runs the application's layout in it,
//! feeds it synthesized pointer and keyboard events, and keeps the resulting
//! paint list so tests can look at what was drawn.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use eframe::egui::{
    self, pos2, vec2, Color32, Event, Modifiers, MouseWheelUnit, PointerButton, Pos2, Rect, Shape,
    TouchPhase, Vec2,
};

use crate::application::{Application, DirectorySource, ServerSource};
use crate::specbase::FileId;
use crate::ui;

/// The window the harness pretends to run in.
pub const SCREEN: Vec2 = vec2(1280.0, 820.0);

/// How far the clock moves per frame. Two clicks inside egui's double click
/// delay have to fit in a handful of frames, so this matches a real 60Hz frame.
const FRAME_TIME: f64 = 1.0 / 60.0;

pub struct Harness {
    app: Application,
    ctx: egui::Context,
    events: Vec<Event>,
    pointer: Pos2,
    time: f64,
    size: Vec2,
    output: egui::FullOutput,
    close_requested: bool,
}

impl Harness {
    /// A harness with nothing open.
    ///
    /// No language server is started: a test that wants one puts a scripted
    /// server behind the editor with [`Harness::language_server`], so nothing
    /// here ever runs a process.
    pub fn new() -> Self {
        let ctx = egui::Context::default();
        let mut app = Application::with_context(&ctx);
        app.set_server_source(ServerSource::None);
        let mut harness = Harness {
            app,
            ctx,
            events: Vec::new(),
            pointer: pos2(-1.0, -1.0),
            time: 0.0,
            size: SCREEN,
            output: egui::FullOutput::default(),
            close_requested: false,
        };
        harness.frame();
        harness
    }

    /// A harness with this project open in it; the project is a Piton project,
    /// which makes it a specbase every test can rely on.
    pub fn with_project() -> Self {
        let mut harness = Harness::new();
        harness.app.open_path(&project_root());
        harness.frames(2);
        harness
    }

    /// Put a scripted language server behind the editor, and hand back the
    /// hold on it: what the editor told it, and what it says next.
    pub fn language_server(&mut self) -> crate::ui::editor::lsp::fake::Server {
        let root = self
            .app
            .open_directory()
            .expect("a directory is open")
            .to_path_buf();
        let (lsp, server) = crate::ui::editor::lsp::fake::Server::start(&root);
        self.app.set_language_server(Some(lsp));
        // The client and the server say hello before anything else happens.
        self.frames(2);
        server
    }

    /// Open a directory, and let the map settle.
    pub fn open(&mut self, dir: &Path) -> &mut Self {
        self.app.open_path(dir);
        self.frames(2)
    }

    /// Answer the next Open Directory requests with these paths.
    pub fn script_directories(&mut self, paths: &[PathBuf]) -> &mut Self {
        self.app
            .set_directory_source(DirectorySource::Scripted(paths.iter().cloned().collect()));
        self
    }

    pub fn app(&self) -> &Application {
        &self.app
    }

    pub fn app_mut(&mut self) -> &mut Application {
        &mut self.app
    }

    pub fn node_map(&self) -> &ui::NodeMap {
        self.app.node_map()
    }

    pub fn screen(&self) -> Rect {
        Rect::from_min_size(Pos2::ZERO, self.size)
    }

    // ------------------------------------------------------------- running

    /// Run one frame of the application.
    pub fn frame(&mut self) -> &mut Self {
        let raw_input = egui::RawInput {
            screen_rect: Some(self.screen()),
            time: Some(self.time),
            events: std::mem::take(&mut self.events),
            focused: true,
            ..Default::default()
        };
        // The same call eframe makes, with the same root `Ui`.
        let app = &mut self.app;
        let mut output = self.ctx.run_ui(raw_input, |ui| ui::layout::show(app, ui));
        // A real backend uploads the new glyphs; nothing here draws pixels, so
        // the deltas are dropped on purpose.
        output.textures_delta.clear();
        self.close_requested |= output
            .viewport_output
            .values()
            .any(|viewport| viewport.commands.contains(&egui::ViewportCommand::Close));
        self.output = output;
        self.time += FRAME_TIME;
        self
    }

    pub fn frames(&mut self, count: usize) -> &mut Self {
        for _ in 0..count {
            self.frame();
        }
        self
    }

    // ------------------------------------------------------------- pointer

    pub fn move_pointer(&mut self, pos: Pos2) -> &mut Self {
        self.pointer = pos;
        self.events.push(Event::PointerMoved(pos));
        self.frame()
    }

    /// Press, release, and let the result settle.
    pub fn click(&mut self, pos: Pos2) -> &mut Self {
        self.end_gesture();
        self.move_pointer(pos);
        self.press();
        self.frame();
        self.release();
        self.frames(2)
    }

    /// Two clicks close enough together for egui to read them as one gesture.
    pub fn double_click(&mut self, pos: Pos2) -> &mut Self {
        self.end_gesture();
        self.move_pointer(pos);
        for _ in 0..2 {
            self.press();
            self.frame();
            self.release();
            self.frame();
        }
        self.frames(2)
    }

    /// Press at `from`, move to `to` in steps, and release.
    pub fn drag(&mut self, from: Pos2, to: Pos2) -> &mut Self {
        self.drag_with(PointerButton::Primary, from, to)
    }

    /// Drag with a button of your own choosing.
    pub fn drag_with(&mut self, button: PointerButton, from: Pos2, to: Pos2) -> &mut Self {
        self.move_pointer(from);
        self.press_button(button);
        self.frame();

        const STEPS: usize = 8;
        for step in 1..=STEPS {
            let t = step as f32 / STEPS as f32;
            self.pointer = from + (to - from) * t;
            self.events.push(Event::PointerMoved(self.pointer));
            self.frame();
        }

        self.release_button(button);
        self.frames(2)
    }

    /// Press and release a button of your own choosing, without moving.
    pub fn click_with(&mut self, button: PointerButton, pos: Pos2) -> &mut Self {
        self.end_gesture();
        self.move_pointer(pos);
        self.press_button(button);
        self.frame();
        self.release_button(button);
        self.frames(2)
    }

    /// Scroll the wheel over `pos`. egui smooths scrolling, so this spreads the
    /// wheel over a few frames and then lets it settle.
    pub fn scroll(&mut self, pos: Pos2, delta: f32) -> &mut Self {
        self.move_pointer(pos);
        for _ in 0..6 {
            self.events.push(Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(0.0, delta),
                phase: TouchPhase::Move,
                modifiers: Modifiers::default(),
            });
            self.frame();
        }
        self.frames(6)
    }

    /// Type into whatever has the keyboard focus.
    pub fn type_text(&mut self, text: &str) -> &mut Self {
        self.events.push(Event::Text(text.to_string()));
        self.frames(2)
    }

    pub fn press_key(&mut self, key: egui::Key) -> &mut Self {
        self.events.push(Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::default(),
        });
        self.frame();
        self.events.push(Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Modifiers::default(),
        });
        self.frames(2)
    }

    /// Move the clock past egui's multiple click delay, so the next press
    /// starts a gesture of its own rather than extending the last one.
    fn end_gesture(&mut self) {
        self.time += 1.0;
        self.frame();
    }

    fn press(&mut self) {
        self.press_button(PointerButton::Primary);
    }

    fn release(&mut self) {
        self.release_button(PointerButton::Primary);
    }

    fn press_button(&mut self, button: PointerButton) {
        self.events.push(Event::PointerButton {
            pos: self.pointer,
            button,
            pressed: true,
            modifiers: Modifiers::default(),
        });
    }

    fn release_button(&mut self, button: PointerButton) {
        self.events.push(Event::PointerButton {
            pos: self.pointer,
            button,
            pressed: false,
            modifiers: Modifiers::default(),
        });
    }

    // ------------------------------------------------------ what was drawn

    /// Every piece of text painted in the last frame, with where it was drawn.
    pub fn texts(&self) -> Vec<(String, Pos2)> {
        let mut out = Vec::new();
        self.visit(&mut |shape| {
            if let Shape::Text(text) = shape {
                out.push((text.galley.text().to_string(), text.pos));
            }
        });
        out
    }

    /// True when some text painted in the last frame contains `needle`.
    pub fn has_text(&self, needle: &str) -> bool {
        self.texts().iter().any(|(text, _)| text.contains(needle))
    }

    /// Where a piece of text was painted, matched in full.
    pub fn text_position(&self, text: &str) -> Option<Pos2> {
        self.texts()
            .into_iter()
            .find(|(painted, _)| painted == text)
            .map(|(_, pos)| pos)
    }

    pub fn text_containing(&self, needle: &str) -> Option<String> {
        self.texts()
            .into_iter()
            .find(|(text, _)| text.contains(needle))
            .map(|(text, _)| text)
    }

    /// Every run of text painted in the last frame, with the colour it was
    /// drawn in and the line under it, if it has one.
    pub fn painted_text(&self) -> Vec<PaintedText> {
        let mut out = Vec::new();
        self.visit(&mut |shape| {
            if let Shape::Text(text) = shape {
                let job = &text.galley.job;
                for section in &job.sections {
                    let range = section.byte_range.start.0..section.byte_range.end.0;
                    let Some(run) = job.text.get(range) else { continue };
                    out.push(PaintedText {
                        text: run.to_string(),
                        color: section.format.color,
                        underline: section.format.underline.color,
                    });
                }
            }
        });
        out
    }

    /// The colour a piece of text was drawn in, the first time it appears.
    pub fn color_of(&self, text: &str) -> Option<Color32> {
        self.painted_text()
            .into_iter()
            .find(|painted| painted.text == text)
            .map(|painted| painted.color)
    }

    /// Every rectangle painted in the last frame, filled or outlined.
    pub fn rects(&self) -> Vec<PaintedRect> {
        let mut out = Vec::new();
        self.visit(&mut |shape| {
            if let Shape::Rect(rect) = shape {
                out.push(PaintedRect {
                    rect: rect.rect,
                    fill: rect.fill,
                    stroke: rect.stroke.color,
                });
            }
        });
        out
    }

    /// How many shapes were painted; a rough measure of how busy the canvas is.
    pub fn shape_count(&self) -> usize {
        let mut count = 0;
        self.visit(&mut |_| count += 1);
        count
    }

    /// True when the application has asked to close the window.
    pub fn close_requested(&self) -> bool {
        self.close_requested
    }

    fn visit(&self, f: &mut dyn FnMut(&Shape)) {
        for clipped in &self.output.shapes {
            visit_shape(&clipped.shape, f);
        }
    }

    // ------------------------------------------------------------ the map

    /// The middle of a node on screen, ready to be clicked.
    pub fn node_center(&self, file: FileId) -> Pos2 {
        self.node_map()
            .screen_rect(file)
            .expect("the canvas is showing this node")
            .center()
    }

    /// A point on the canvas that is not on any node.
    pub fn empty_canvas_point(&self) -> Pos2 {
        let viewport = self.node_map().viewport();
        let mut candidate = viewport.center();
        let step = vec2(0.0, 6.0);
        for _ in 0..200 {
            let taken = self
                .node_map()
                .visible_files()
                .into_iter()
                .filter_map(|file| self.node_map().screen_rect(file))
                .any(|rect| rect.expand(8.0).contains(candidate));
            if !taken && viewport.shrink(20.0).contains(candidate) {
                return candidate;
            }
            candidate += step;
            if !viewport.contains(candidate) {
                candidate = pos2(candidate.x + 20.0, viewport.top() + 24.0);
            }
        }
        panic!("no empty spot on the canvas");
    }

    // -------------------------------------------------------- the sidebar

    /// The room the editor sidebar takes while it is out: whatever is left of
    /// the body once the canvas has taken its share. Measured rather than
    /// assumed, because the edge between the two can be dragged.
    pub fn sidebar(&self) -> Rect {
        let canvas = self.node_map().viewport();
        Rect::from_min_max(
            pos2(canvas.right(), canvas.top()),
            pos2(self.screen().right(), canvas.bottom()),
        )
    }

    /// The edge between the canvas and the sidebar, which can be dragged.
    pub fn sidebar_edge(&self) -> Pos2 {
        let sidebar = self.sidebar();
        pos2(sidebar.left(), sidebar.center().y)
    }

    /// A point inside the text of the editor, ready to be clicked.
    ///
    /// The editor draws its buffer as one piece of text, so this is where that
    /// text begins.
    pub fn editor_text_point(&self) -> Pos2 {
        let text = self
            .app
            .editor()
            .expect("the sidebar is out")
            .text()
            .to_string();
        let (_, pos) = self
            .texts()
            .into_iter()
            .find(|(painted, _)| *painted == text)
            .expect("the editor draws the text of the file");
        pos + vec2(2.0, 6.0)
    }

    /// The file with the given path, relative to the root of the specbase.
    pub fn file_id(&self, rel: &str) -> FileId {
        let specbase = self.app.specbase().expect("a specbase is open");
        specbase
            .files
            .iter()
            .find(|file| file.rel == rel)
            .unwrap_or_else(|| panic!("no file called {rel}"))
            .id
    }
}

/// A run of text as it was laid out.
pub struct PaintedText {
    pub text: String,
    pub color: Color32,
    /// The colour of the line under it; transparent when it has none.
    pub underline: Color32,
}

/// A rectangle as it was painted.
pub struct PaintedRect {
    pub rect: Rect,
    pub fill: Color32,
    pub stroke: Color32,
}

fn visit_shape(shape: &Shape, f: &mut dyn FnMut(&Shape)) {
    f(shape);
    if let Shape::Vec(shapes) = shape {
        for shape in shapes {
            visit_shape(shape, f);
        }
    }
}

/// The root of this project, which is itself a Piton project.
pub fn project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// A small Piton project of its own, in the system's temporary directory.
///
/// Tests that write files write into one of these, never into the project the
/// rest of the tests read.
pub fn scratch_project() -> PathBuf {
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "overlook-{}-{}",
        std::process::id(),
        COUNT.fetch_add(1, Ordering::Relaxed)
    ));
    let spec = dir.join("spec");
    std::fs::create_dir_all(&spec).unwrap();
    std::fs::write(
        dir.join("piton.config.pi"),
        "export piton-config Project:\n    root: ./spec\n    entry: ./spec/index.pi\n",
    )
    .unwrap();
    std::fs::write(spec.join("index.pi"), "from ./Thing export *\n").unwrap();
    std::fs::write(
        spec.join("Thing.pi"),
        "export anchor Thing:\n    pitch: A thing worth exploring.\n",
    )
    .unwrap();
    dir
}
