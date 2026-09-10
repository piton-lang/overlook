//! An editor for Piton files, with syntax highlighting and an integration with
//! the Piton language server.
//!
//! A small header on top carries a Save and a Cancel button on the left and a
//! close button on the right; the text of the file sits underneath it,
//! coloured by the language's own grammar and marked with whatever the
//! language server has to say about it.
//!
//! The editor is what the node map's sidebar puts on screen for the selected
//! file.

pub mod highlight;
pub mod lsp;

use std::fs;
use std::path::{Path, PathBuf};

use eframe::egui::{self, Align, FontId, Layout, RichText, Stroke, Ui};

use highlight::{Highlighter, Mark};
pub use lsp::Lsp;
use lsp::{Problem, Severity, Status};

use crate::specbase::FileId;

use super::icon::Icon;
use super::theme;
use super::toolbar::{Toolbar, ToolbarButton, ToolbarGroup};

/// The size the file itself is drawn at.
const FONT_SIZE: f32 = 12.5;

/// How many of a file's problems are listed before they are counted instead.
const PROBLEMS_SHOWN: usize = 4;

/// How long the pointer rests on a word before the server is asked about it.
const HOVER_DELAY: f32 = 0.35;

/// What the header was asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    Save,
    Cancel,
    Close,
}

/// One open file, and the edits made to it since it was opened or saved.
pub struct Editor {
    file: FileId,
    path: PathBuf,
    /// The path of the file relative to the root of the specbase.
    rel: String,
    /// The text being edited.
    text: String,
    /// The text as it was last read from, or written to, disk.
    saved: String,
    /// Set when the file could not be read or written.
    error: Option<String>,
    highlighter: Highlighter,
}

impl Editor {
    /// Open a file, reading it off disk.
    pub fn open(file: FileId, path: &Path, rel: &str) -> Self {
        let (text, error) = match fs::read_to_string(path) {
            Ok(text) => (text, None),
            Err(error) => (String::new(), Some(format!("Could not read {rel}: {error}"))),
        };
        Editor {
            file,
            path: path.to_path_buf(),
            rel: rel.to_string(),
            saved: text.clone(),
            text,
            error,
            highlighter: Highlighter::default(),
        }
    }

    /// The file being edited.
    pub fn file(&self) -> FileId {
        self.file
    }

    // The three accessors below describe the open file; the editor draws
    // itself from its own fields, and the tests read it through these.

    #[allow(dead_code)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn rel(&self) -> &str {
        &self.rel
    }

    #[allow(dead_code)]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// True while the buffer holds edits that are not on disk.
    pub fn is_dirty(&self) -> bool {
        self.text != self.saved
    }

    #[allow(dead_code)]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Write the buffer back to the file.
    pub fn save(&mut self) -> Result<(), String> {
        match fs::write(&self.path, &self.text) {
            Ok(()) => {
                self.saved = self.text.clone();
                self.error = None;
                Ok(())
            }
            Err(error) => {
                let message = format!("Could not save {}: {error}", self.rel);
                self.error = Some(message.clone());
                Err(message)
            }
        }
    }

    /// Throw the unsaved edits away and go back to what is on disk.
    pub fn cancel(&mut self) {
        self.text = self.saved.clone();
    }

    /// Draw the editor and report what its header was asked to do.
    ///
    /// The language server, when there is one, is told what the buffer holds
    /// and asked about what the pointer is resting on.
    pub fn show(&mut self, ui: &mut Ui, mut lsp: Option<&mut Lsp>) -> Option<EditorAction> {
        // The server works from what the editor is showing, not from what is
        // on disk, so it hears about the buffer before anything is drawn.
        if let Some(lsp) = lsp.as_deref_mut() {
            lsp.document(&self.path, &self.text);
        }

        let action = self.header(ui);
        self.name_strip(ui, lsp.as_deref());

        let problems: Vec<Problem> = match &lsp {
            Some(lsp) => lsp.problems(&self.path).to_vec(),
            None => Vec::new(),
        };
        // The problems sit at the foot of the sidebar, so they take their room
        // before the text is given what is left.
        self.problems_strip(ui, &problems);
        self.body(ui, &problems, lsp);
        action
    }

    // ------------------------------------------------------------- header

    fn header(&self, ui: &mut Ui) -> Option<EditorAction> {
        let dirty = self.is_dirty();
        let mut action = None;

        let frame = egui::Frame::new()
            .fill(theme::PANEL)
            .inner_margin(egui::Margin::symmetric(4, 4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let left = Toolbar::new(vec![ToolbarGroup::new(vec![
                        ToolbarButton::icon_text("save", Icon::Save, "Save")
                            .enabled(dirty)
                            .tooltip("Write the changes back to the file"),
                        ToolbarButton::icon_text("cancel", Icon::Revert, "Cancel")
                            .enabled(dirty)
                            .tooltip("Throw the unsaved changes away"),
                    ])]);
                    match left.show(ui) {
                        Some("save") => action = Some(EditorAction::Save),
                        Some("cancel") => action = Some(EditorAction::Cancel),
                        _ => {}
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let close = Toolbar::new(vec![ToolbarGroup::new(vec![
                            ToolbarButton::icon("close", Icon::Close).tooltip("Close the editor"),
                        ])]);
                        if close.show(ui) == Some("close") {
                            action = Some(EditorAction::Close);
                        }
                    });
                });
            });

        let rect = frame.response.rect;
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(1.0, theme::BORDER),
        );
        action
    }

    /// The file the editor is on, whether it has unsaved edits, and whether
    /// there is a language server behind it.
    fn name_strip(&self, ui: &mut Ui, lsp: Option<&Lsp>) {
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.label(
                RichText::new(&self.rel)
                    .color(theme::TEXT_DIM)
                    .size(11.5)
                    .monospace(),
            );
            if self.is_dirty() {
                ui.label(RichText::new("unsaved").color(theme::WARN).size(11.0));
            }
            // A server that is answering says so by answering; one that is not
            // is worth a word, so its silence is not taken for a clean file.
            let server = match lsp.map(Lsp::status) {
                Some(Status::Starting) => Some(("starting the language server", theme::TEXT_FAINT)),
                Some(Status::Unavailable(_)) | None => {
                    Some(("no language server", theme::TEXT_FAINT))
                }
                Some(Status::Ready) => None,
            };
            if let Some((text, color)) = server {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(4.0);
                    ui.label(RichText::new(text).color(color).size(11.0))
                        .on_hover_text(match lsp.map(Lsp::status) {
                            Some(Status::Unavailable(why)) => why.clone(),
                            _ => format!("`{} {}`", lsp::SERVER_COMMAND, lsp::SERVER_ARGS.join(" ")),
                        });
                });
            }
        });
        if let Some(error) = &self.error {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(4.0);
                ui.label(RichText::new(error).color(theme::DANGER).size(11.5));
            });
        }
        ui.add_space(2.0);
    }

    // ----------------------------------------------------------- problems

    /// What the language server has to say about the file, at its foot.
    fn problems_strip(&self, ui: &mut Ui, problems: &[Problem]) {
        if problems.is_empty() {
            return;
        }
        egui::Panel::bottom("editor_problems")
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin::symmetric(6, 4)),
            )
            .show(ui, |ui| {
                ui.painter().line_segment(
                    [ui.max_rect().left_top(), ui.max_rect().right_top()],
                    Stroke::new(1.0, theme::BORDER),
                );
                ui.spacing_mut().item_spacing.y = 2.0;
                for problem in problems.iter().take(PROBLEMS_SHOWN) {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;
                        ui.label(
                            RichText::new(format!("{}", problem.start.line + 1))
                                .color(theme::TEXT_FAINT)
                                .size(11.0)
                                .monospace(),
                        );
                        ui.label(
                            RichText::new(&problem.message)
                                .color(severity_color(problem.severity))
                                .size(11.5),
                        );
                    });
                }
                if problems.len() > PROBLEMS_SHOWN {
                    ui.label(
                        RichText::new(format!("and {} more", problems.len() - PROBLEMS_SHOWN))
                            .color(theme::TEXT_FAINT)
                            .size(11.0),
                    );
                }
            });
    }

    // --------------------------------------------------------------- body

    fn body(&mut self, ui: &mut Ui, problems: &[Problem], lsp: Option<&mut Lsp>) {
        // The text and the scroll it sits in are named after the file they are
        // on rather than by where they fall in the sidebar. What the server has
        // to say comes and goes as the file is typed at, and the strip that
        // says it takes room from the sidebar; a widget left with the id its
        // position gives it would be a different widget every time that
        // happened, losing the keyboard, the cursor and the place in the file
        // with it. Naming them after the file also keeps one file's cursor from
        // turning up in the next one.
        let text_id = egui::Id::new(("editor_text", &self.path));
        let scroll_id = ("editor_scroll", self.path.clone());

        let readable = self.error.is_none();
        let marks: Vec<Mark> = problems
            .iter()
            .map(|problem| Mark {
                range: problem.range_in(&self.text),
                color: severity_color(problem.severity),
            })
            .collect();

        let highlighter = &mut self.highlighter;
        let text = &mut self.text;
        let font = FontId::monospace(FONT_SIZE);
        let mut layouter = |ui: &Ui, buffer: &dyn egui::TextBuffer, wrap_width: f32| {
            let job = highlighter.job(buffer.as_str(), font.clone(), wrap_width, &marks);
            ui.fonts_mut(|fonts| fonts.layout_job(job))
        };

        let output = egui::Frame::new()
            .fill(theme::BACKDROP)
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                egui::ScrollArea::both()
                    .id_salt(scroll_id)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_enabled_ui(readable, |ui| {
                            egui::TextEdit::multiline(text)
                                .id(text_id)
                                .code_editor()
                                .desired_width(f32::INFINITY)
                                .frame(egui::Frame::NONE)
                                .layouter(&mut layouter)
                                .show(ui)
                        })
                        .inner
                    })
                    .inner
            })
            .inner;

        if let Some(lsp) = lsp {
            self.hover(ui, &output, lsp);
        }
    }

    /// Ask the server what the pointer is resting on, and say what it answered.
    fn hover(&self, ui: &Ui, output: &egui::text_edit::TextEditOutput, lsp: &mut Lsp) {
        let Some(pointer) = output.response.response.hover_pos() else {
            return;
        };
        if !output.text_clip_rect.contains(pointer) {
            return;
        }
        // Asking for every pixel the pointer crosses would be a request a
        // frame; a pointer that has come to rest is one that is asking.
        if ui.input(|input| input.pointer.time_since_last_movement()) < HOVER_DELAY {
            return;
        }

        let cursor = output.galley.cursor_from_pos(pointer - output.galley_pos);
        let offset = self
            .text
            .char_indices()
            .nth(cursor.index.0)
            .map(|(offset, _)| offset)
            .unwrap_or(self.text.len());
        let at = lsp::position_of(&self.text, offset);

        lsp.ask_hover(&self.path, at);
        if let Some(answer) = lsp.hover(&self.path, at) {
            output.response.response.clone().show_tooltip_text(
                RichText::new(answer).size(11.5).monospace(),
            );
        }
    }
}

/// The colour a problem of this severity is drawn in.
fn severity_color(severity: Severity) -> egui::Color32 {
    match severity {
        Severity::Error => theme::DANGER,
        Severity::Warning => theme::WARN,
        Severity::Note => theme::ACCENT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file of its own for one test, in the system's temporary directory.
    fn scratch_file(name: &str, contents: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "overlook-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn opening_a_file_reads_it_and_starts_clean() {
        let path = scratch_file("Thing.pi", "export anchor Thing:\n");
        let editor = Editor::open(0, &path, "spec/Thing.pi");

        assert_eq!(editor.text(), "export anchor Thing:\n");
        assert_eq!(editor.rel(), "spec/Thing.pi");
        assert_eq!(editor.file(), 0);
        assert!(!editor.is_dirty(), "an untouched file has nothing to save");
        assert!(editor.error().is_none());
    }

    #[test]
    fn a_file_that_cannot_be_read_says_so() {
        let editor = Editor::open(0, Path::new("/nowhere/Missing.pi"), "Missing.pi");

        assert!(editor.text().is_empty());
        assert!(
            editor.error().unwrap().contains("Missing.pi"),
            "the error names the file"
        );
        assert!(!editor.is_dirty());
    }

    #[test]
    fn editing_makes_the_editor_dirty_and_saving_writes_the_file() {
        let path = scratch_file("Thing.pi", "one\n");
        let mut editor = Editor::open(0, &path, "Thing.pi");

        editor.text.push_str("two\n");
        assert!(editor.is_dirty());

        editor.save().expect("the file was written");
        assert_eq!(fs::read_to_string(&path).unwrap(), "one\ntwo\n");
        assert!(!editor.is_dirty(), "what is on disk needs no saving");
    }

    #[test]
    fn cancelling_goes_back_to_what_is_on_disk() {
        let path = scratch_file("Thing.pi", "one\n");
        let mut editor = Editor::open(0, &path, "Thing.pi");

        editor.text = String::from("something else\n");
        editor.cancel();

        assert_eq!(editor.text(), "one\n");
        assert!(!editor.is_dirty());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "one\n",
            "cancelling never touches the file"
        );
    }

    #[test]
    fn saving_a_file_that_cannot_be_written_reports_the_failure() {
        let mut editor = Editor::open(0, Path::new("/nowhere/Missing.pi"), "Missing.pi");
        editor.text = String::from("anything");

        let error = editor.save().expect_err("there is nowhere to write");
        assert!(error.contains("Missing.pi"));
        assert_eq!(editor.error(), Some(error.as_str()));
        assert!(editor.is_dirty(), "a failed save leaves the edits alone");
    }
}
