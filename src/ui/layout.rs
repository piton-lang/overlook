//! The application layout: a toolbar header, the node map body, and a status
//! bar footer.
//!
//! The node map's editor sidebar is part of the body: it takes its room from
//! the canvas, between the header and the footer.

use eframe::egui::{self, vec2, Stroke};

use crate::application::{Application, MessageKind, APP_DESCRIPTION, APP_NAME, APP_VERSION};

use super::icon::Icon;
use super::node_map;
use super::status_bar::{StatusBar, StatusGroup, StatusItem};
use super::theme;
use super::toolbar::{Toolbar, ToolbarButton, ToolbarGroup};

pub fn show(app: &mut Application, ui: &mut egui::Ui) {
    // The sidebar follows the selection the canvas left behind last frame.
    app.sync_editor();
    header(app, ui);
    footer(app, ui);
    sidebar(app, ui);
    body(app, ui);
    about(app, ui.ctx());
}

// ------------------------------------------------------------------- header

fn header(app: &mut Application, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    egui::Panel::top("header")
        .exact_size(theme::HEADER_HEIGHT)
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .inner_margin(egui::Margin::symmetric(6, 0)),
        )
        .show(ui, |ui| {
            ui.painter().line_segment(
                [ui.max_rect().left_bottom(), ui.max_rect().right_bottom()],
                Stroke::new(1.0, theme::BORDER),
            );
            let toolbar = Toolbar::new(vec![
                ToolbarGroup::new(vec![ToolbarButton::icon_text(
                    "open_directory",
                    Icon::FolderOpen,
                    "Open Directory",
                )
                .tooltip(format!(
                    "Open a directory containing {}",
                    crate::specbase::CONFIG_FILE
                ))]),
                ToolbarGroup::new(vec![
                    ToolbarButton::icon_text("about", Icon::Info, "About"),
                    ToolbarButton::icon_text("exit", Icon::Power, "Exit").danger(true),
                ]),
            ]);
            ui.vertical_centered_justified(|ui| {
                ui.add_space((theme::HEADER_HEIGHT - toolbar.button_height) / 2.0);
                match toolbar.show(ui) {
                    Some("open_directory") => app.pick_directory(),
                    Some("about") => app.set_about_open(!app.about_open()),
                    Some("exit") => Application::exit(&ctx),
                    _ => {}
                }
            });
        });
}

// ------------------------------------------------------------------ sidebar

/// The editor for the selected node, beside the canvas.
fn sidebar(app: &mut Application, ui: &mut egui::Ui) {
    let action = match app.sidebar() {
        Some((editor, lsp)) => node_map::sidebar(ui, editor, lsp),
        None => None,
    };
    if let Some(action) = action {
        app.apply_editor_action(action);
    }
}

// --------------------------------------------------------------------- body

fn body(app: &mut Application, ui: &mut egui::Ui) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::CANVAS))
        .show(ui, |ui| {
            let (node_map, specbase) = app.canvas();
            node_map.show(ui, specbase);
        });
}

// ------------------------------------------------------------------- footer

fn footer(app: &mut Application, ui: &mut egui::Ui) {
    egui::Panel::bottom("footer")
        .exact_size(theme::FOOTER_HEIGHT)
        .frame(egui::Frame::new().fill(theme::PANEL))
        .show(ui, |ui| {
            ui.painter().line_segment(
                [ui.max_rect().left_top(), ui.max_rect().right_top()],
                Stroke::new(1.0, theme::BORDER),
            );
            status_bar(app).show(ui);
        });
}

fn status_bar(app: &Application) -> StatusBar {
    let mut bar = StatusBar::default();

    // Far left: the absolute path of the currently open directory.
    let far_left = match app.open_directory() {
        Some(path) => StatusItem::icon_text(Icon::FolderOpen, path.display().to_string())
            .color(theme::TEXT)
            .tooltip("Currently open directory"),
        None => StatusItem::icon_text(Icon::FolderOpen, "No directory open")
            .color(theme::TEXT_FAINT),
    };
    bar.left.push(StatusGroup::new(vec![far_left]));

    if let Some(specbase) = app.specbase() {
        bar.left.push(StatusGroup::new(vec![
            StatusItem::text(format!("{} files", specbase.files.len())),
            StatusItem::text(format!("{} declarations", specbase.symbol_count())),
        ]));

        let unreached_files = specbase.unreached_files();
        let unreached_symbols = specbase.unreached_symbols();
        if unreached_files + unreached_symbols > 0 {
            bar.left.push(StatusGroup::new(vec![StatusItem::icon_text(
                Icon::Warning,
                format!("{unreached_files} files, {unreached_symbols} declarations unreached"),
            )
            .color(theme::WARN)]));
        }
    }

    if let Some(message) = app.message() {
        if message.kind == MessageKind::Error {
            bar.left.push(StatusGroup::new(vec![
                StatusItem::icon_text(Icon::Warning, message.text.clone()).color(theme::DANGER),
            ]));
        }
    }

    // Focus is a mode the whole canvas is in, so it belongs in the status bar.
    if let (Some(specbase), Some(file)) = (app.specbase(), app.focused_file()) {
        bar.left.push(StatusGroup::new(vec![StatusItem::icon_text(
            Icon::Focus,
            format!("Focused on {}", specbase.file(file).name),
        )
        .color(theme::ACCENT)
        .tooltip("Press Escape to leave focus")]));
    }

    if let Some((specbase, file)) = app.selected_file() {
        let file = specbase.file(file);
        bar.right.push(StatusGroup::new(vec![
            StatusItem::icon_text(Icon::File, file.rel.clone()).color(theme::TEXT),
            StatusItem::text(format!("{} declarations", file.symbols.len())),
        ]));
    }

    if app.specbase().is_some() {
        bar.right.push(StatusGroup::new(vec![StatusItem::text(format!(
            "{:.0}%",
            app.node_map().zoom() * 100.0
        ))]));
    }

    bar
}

// -------------------------------------------------------------------- about

fn about(app: &mut Application, ctx: &egui::Context) {
    let mut open = app.about_open();
    if !open {
        return;
    }
    egui::Window::new("About")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_min_width(340.0);
            ui.add_space(4.0);
            ui.heading(APP_NAME);
            ui.add_space(2.0);
            ui.label(egui::RichText::new(APP_DESCRIPTION).color(theme::TEXT_DIM));
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(format!("Version {APP_VERSION}")).color(theme::TEXT_FAINT),
            );
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Node map").color(theme::TEXT));
            ui.label(
                egui::RichText::new(
                    "Drag the canvas to pan, or drag with the middle button from \
                     anywhere, including from a node. Scroll to zoom, drag a node \
                     to move it, click a node to select it. Double click a node to \
                     focus on it and its connections; Escape, or a double click on \
                     the background, leaves focus.",
                )
                .color(theme::TEXT_DIM),
            );
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Editor").color(theme::TEXT));
            ui.label(
                egui::RichText::new(
                    "Selecting a node opens its file in the sidebar, coloured by \
                     the Piton grammar. Save writes the changes back, Cancel \
                     throws them away, and the close button lets the node go. \
                     The language server marks what it finds wrong at the foot \
                     of the sidebar, and says what a word is when the pointer \
                     rests on it. Drag the edge between the map and the sidebar \
                     to say how the two share the room.",
                )
                .color(theme::TEXT_DIM),
            );
            ui.add_space(4.0);
        });
    app.set_about_open(open);
}
