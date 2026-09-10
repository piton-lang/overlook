//! An application for exploring Piton specifications.
//!
//! Open a directory that contains a `piton.config.pi` and explore the structure
//! of the entire spec through the node map.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use eframe::egui;

use crate::specbase::{FileId, Specbase};
use crate::ui::{self, Editor, EditorAction, Lsp};

/// The name the shape gives the application.
pub const APP_NAME: &str = "Overlook";
pub const APP_DESCRIPTION: &str = "An application for exploring Piton specifications.";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Info,
    Error,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub text: String,
    pub kind: MessageKind,
}

/// Where the language server comes from.
///
/// The application runs the one the language ships. Tests put a server of
/// their own in its place, so no process is started to test the editor.
pub enum ServerSource {
    Piton,
    #[allow(dead_code)] // Used by the testing harness.
    None,
}

impl Default for ServerSource {
    fn default() -> Self {
        ServerSource::Piton
    }
}

/// Where the directory to open comes from.
///
/// The application asks the desktop for one. Tests script the answer instead,
/// so the whole open flow runs without a native dialog.
#[derive(Default)]
#[allow(dead_code)] // `Scripted` is how the tests answer the dialog.
pub enum DirectorySource {
    #[default]
    Dialog,
    Scripted(VecDeque<PathBuf>),
}

#[derive(Default)]
pub struct Application {
    specbase: Option<Specbase>,
    node_map: ui::NodeMap,
    /// The file open in the sidebar, which is the selected node's file.
    editor: Option<Editor>,
    /// The language server for the directory that is open.
    lsp: Option<Lsp>,
    server_source: ServerSource,
    about_open: bool,
    message: Option<Message>,
    directory_source: DirectorySource,
}

impl Application {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Application::with_context(&cc.egui_ctx)
    }

    /// Build an application against a context, styling it on the way.
    pub fn with_context(ctx: &egui::Context) -> Self {
        ui::theme::install(ctx);
        Application::default()
    }

    /// Answer the next directory requests from a list instead of a dialog.
    #[allow(dead_code)] // Used by the testing harness.
    pub fn set_directory_source(&mut self, source: DirectorySource) {
        self.directory_source = source;
    }

    /// Say where the language server comes from, before a directory is opened.
    #[allow(dead_code)] // Used by the testing harness.
    pub fn set_server_source(&mut self, source: ServerSource) {
        self.server_source = source;
    }

    /// Put a language server behind the editor.
    #[allow(dead_code)] // Used by the testing harness.
    pub fn set_language_server(&mut self, lsp: Option<Lsp>) {
        self.lsp = lsp;
    }

    /// The language server, when there is one.
    #[allow(dead_code)] // The tests read the session through this.
    pub fn language_server(&self) -> Option<&Lsp> {
        self.lsp.as_ref()
    }

    /// Open a directory straight away, e.g. one named on the command line.
    pub fn with_directory(mut self, dir: &Path) -> Self {
        self.open_path(dir);
        self
    }

    pub fn specbase(&self) -> Option<&Specbase> {
        self.specbase.as_ref()
    }

    /// The node map and the specbase it draws, borrowed together.
    pub fn canvas(&mut self) -> (&mut ui::NodeMap, Option<&Specbase>) {
        (&mut self.node_map, self.specbase.as_ref())
    }

    pub fn node_map(&self) -> &ui::NodeMap {
        &self.node_map
    }

    pub fn about_open(&self) -> bool {
        self.about_open
    }

    pub fn set_about_open(&mut self, open: bool) {
        self.about_open = open;
    }

    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref()
    }

    /// The absolute path of the currently open directory.
    pub fn open_directory(&self) -> Option<&Path> {
        self.specbase.as_ref().map(|s| s.root.as_path())
    }

    /// The editor and the server it talks to, borrowed together.
    pub fn sidebar(&mut self) -> Option<(&mut Editor, Option<&mut Lsp>)> {
        let editor = self.editor.as_mut()?;
        Some((editor, self.lsp.as_mut()))
    }

    /// Open, swap or close the editor so it follows the selected node, and take
    /// in whatever the language server has said since the last frame.
    ///
    /// A single selected node brings the sidebar out with that node's file in
    /// it; letting the selection go takes the sidebar away again.
    pub fn sync_editor(&mut self) {
        if let Some(lsp) = self.lsp.as_mut() {
            lsp.poll();
        }
        let Some(specbase) = self.specbase.as_ref() else {
            self.close_editor();
            return;
        };
        let opening = match self.node_map.selected() {
            Some(file) if self.editor.as_ref().map(Editor::file) != Some(file) => {
                let spec_file = specbase.file(file);
                Some((file, spec_file.path.clone(), spec_file.rel.clone()))
            }
            Some(_) => return,
            None => None,
        };
        self.close_editor();
        if let Some((file, path, rel)) = opening {
            self.editor = Some(Editor::open(file, &path, &rel));
        }
    }

    /// Put the open editor away, telling the server the file is closed.
    fn close_editor(&mut self) {
        let Some(editor) = self.editor.take() else {
            return;
        };
        if let Some(lsp) = self.lsp.as_mut() {
            lsp.close(editor.path());
        }
    }

    /// The editor in the sidebar, when the sidebar is out.
    #[allow(dead_code)] // The tests read the sidebar through this.
    pub fn editor(&self) -> Option<&Editor> {
        self.editor.as_ref()
    }

    /// Carry out what the editor's header was asked to do.
    pub fn apply_editor_action(&mut self, action: EditorAction) {
        match action {
            EditorAction::Save => {
                let Some(editor) = self.editor.as_mut() else { return };
                let saved = editor.save();
                self.message = Some(match saved {
                    Ok(()) => {
                        // The server reads the file itself; it is told the
                        // file it was given is now the file on disk.
                        if let Some(lsp) = self.lsp.as_mut() {
                            lsp.saved(editor.path(), editor.text());
                        }
                        Message {
                            text: format!("Saved {}", editor.rel()),
                            kind: MessageKind::Info,
                        }
                    }
                    Err(error) => Message {
                        text: error,
                        kind: MessageKind::Error,
                    },
                });
            }
            EditorAction::Cancel => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.cancel();
                }
            }
            // Closing the sidebar means letting the node go: the sidebar is out
            // for as long as a single node is selected.
            EditorAction::Close => {
                self.node_map.clear_selection();
                self.close_editor();
            }
        }
    }

    pub fn selected_file(&self) -> Option<(&Specbase, FileId)> {
        let specbase = self.specbase.as_ref()?;
        let file = self.node_map.selected().or_else(|| self.node_map.hovered())?;
        Some((specbase, file))
    }

    /// Ask for a directory, then open it.
    pub fn pick_directory(&mut self) {
        let picked = match &mut self.directory_source {
            DirectorySource::Dialog => {
                let mut dialog = rfd::FileDialog::new().set_title("Open a Piton project");
                if let Some(current) = self.specbase.as_ref().map(|s| s.root.clone()) {
                    dialog = dialog.set_directory(current);
                }
                dialog.pick_folder()
            }
            DirectorySource::Scripted(queue) => queue.pop_front(),
        };
        if let Some(dir) = picked {
            self.open_path(&dir);
        }
    }

    /// The file the node map is focused on.
    pub fn focused_file(&self) -> Option<FileId> {
        self.node_map.focused()
    }

    pub fn open_path(&mut self, dir: &Path) {
        match Specbase::load(dir) {
            Ok(specbase) => {
                self.node_map.set_specbase(&specbase);
                self.editor = None;
                // The server is started for the project that is open; the one
                // that was open is left behind with it.
                self.lsp = match self.server_source {
                    ServerSource::Piton => Some(Lsp::start(&specbase.root)),
                    ServerSource::None => None,
                };
                self.message = Some(Message {
                    text: format!(
                        "{} files, {} declarations",
                        specbase.files.len(),
                        specbase.symbol_count()
                    ),
                    kind: MessageKind::Info,
                });
                self.specbase = Some(specbase);
            }
            Err(error) => {
                self.message = Some(Message {
                    text: error.to_string(),
                    kind: MessageKind::Error,
                });
            }
        }
    }

    pub fn exit(ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

impl eframe::App for Application {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui::layout::show(self, ui);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let [r, g, b, a] = ui::theme::BACKDROP.to_array();
        [
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        ]
    }
}

/// A directory passed on the command line, if any.
pub fn directory_from_args() -> Option<PathBuf> {
    std::env::args().nth(1).map(PathBuf::from)
}
