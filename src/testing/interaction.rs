//! Interaction tests: what the application does when it is used.
//!
//! These press buttons, drag nodes and type keys through the harness, and check
//! what the application did, not how it looks.

use eframe::egui::{vec2, Key, PointerButton};

use super::harness::{project_root, scratch_project, Harness};
use crate::ui::theme;

// ------------------------------------------------------------------- toolbar

#[test]
fn the_open_directory_button_opens_a_directory() {
    let mut harness = Harness::new();
    harness.script_directories(&[project_root()]);
    assert!(harness.app().specbase().is_none());

    let button = harness
        .texts()
        .into_iter()
        .find(|(text, _)| text == "Open Directory")
        .expect("the header offers Open Directory");
    harness.click(button.1 + vec2(20.0, 6.0));

    let specbase = harness.app().specbase().expect("the directory opened");
    assert_eq!(specbase.root, project_root().canonicalize().unwrap());
}

#[test]
fn the_about_button_toggles_the_about_window() {
    let mut harness = Harness::new();
    let button = harness
        .texts()
        .into_iter()
        .find(|(text, _)| text == "About")
        .expect("the header offers About")
        .1;

    harness.click(button + vec2(12.0, 6.0));
    assert!(harness.app().about_open());

    harness.click(button + vec2(12.0, 6.0));
    assert!(!harness.app().about_open());
}

#[test]
fn the_exit_button_asks_to_close_the_window() {
    let mut harness = Harness::new();
    let button = harness
        .texts()
        .into_iter()
        .find(|(text, _)| text == "Exit")
        .expect("the header offers Exit")
        .1;

    assert!(!harness.close_requested());
    harness.click(button + vec2(10.0, 6.0));
    assert!(harness.close_requested());
}

// ------------------------------------------------------------------ the map

#[test]
fn clicking_a_node_selects_it() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/shape/ui/Layout.pi");

    harness.click(harness.node_center(file));
    assert_eq!(harness.node_map().selected(), Some(file));

    let empty = harness.empty_canvas_point();
    harness.click(empty);
    assert_eq!(harness.node_map().selected(), None);
}

#[test]
fn dragging_a_node_moves_that_node_and_leaves_the_others() {
    let mut harness = Harness::with_project();
    let dragged = harness.file_id("spec/shape/ui/Layout.pi");
    let other = harness.file_id("spec/shape/ui/Toolbar.pi");

    let before = harness.node_map().screen_rect(dragged).unwrap();
    let other_before = harness.node_map().screen_rect(other).unwrap();

    let start = before.center();
    harness.drag(start, start + vec2(120.0, 60.0));

    let after = harness.node_map().screen_rect(dragged).unwrap();
    let moved = after.center() - before.center();
    assert!(
        (moved.x - 120.0).abs() < 2.0 && (moved.y - 60.0).abs() < 2.0,
        "the node followed the pointer, moved by {moved:?}"
    );
    assert_eq!(
        harness.node_map().screen_rect(other).unwrap().center(),
        other_before.center(),
        "the other nodes stayed where they were"
    );
}

#[test]
fn dragging_the_background_pans_every_node() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/index.pi");
    let before = harness.node_map().screen_rect(file).unwrap().center();

    let start = harness.empty_canvas_point();
    harness.drag(start, start + vec2(-80.0, 40.0));

    let after = harness.node_map().screen_rect(file).unwrap().center();
    let moved = after - before;
    assert!(
        (moved.x + 80.0).abs() < 2.0 && (moved.y - 40.0).abs() < 2.0,
        "the whole canvas moved with the pointer, moved by {moved:?}"
    );
}

#[test]
fn the_middle_button_pans_the_canvas_from_wherever_it_is_pressed() {
    let mut harness = Harness::with_project();
    let under_the_pointer = harness.file_id("spec/shape/ui/Layout.pi");
    let other = harness.file_id("spec/shape/ui/Toolbar.pi");
    let before = harness.node_map().screen_rect(under_the_pointer).unwrap();
    let other_before = harness.node_map().screen_rect(other).unwrap().center();

    // Starting on a node: the middle button moves the map, not the node.
    let start = before.center();
    harness.drag_with(PointerButton::Middle, start, start + vec2(-90.0, 50.0));

    let moved = harness.node_map().screen_rect(under_the_pointer).unwrap().center() - before.center();
    assert!(
        (moved.x + 90.0).abs() < 2.0 && (moved.y - 50.0).abs() < 2.0,
        "the map moved with the pointer, by {moved:?}"
    );
    let other_moved = harness.node_map().screen_rect(other).unwrap().center() - other_before;
    assert!(
        (other_moved - moved).length() < 2.0,
        "every node moved together, so the map was panned rather than a node dragged"
    );
}

#[test]
fn the_middle_button_leaves_the_selection_where_it_is() {
    let mut harness = Harness::with_project();
    let selected = harness.file_id("spec/shape/ui/Layout.pi");
    let other = harness.file_id("spec/shape/ui/Toolbar.pi");
    harness.click(harness.node_center(selected));

    harness.click_with(PointerButton::Middle, harness.node_center(other));

    assert_eq!(
        harness.node_map().selected(),
        Some(selected),
        "a middle click is for moving the map, not for choosing a node"
    );
}

#[test]
fn scrolling_zooms_the_canvas() {
    let mut harness = Harness::with_project();
    let before = harness.node_map().zoom();

    let point = harness.empty_canvas_point();
    harness.scroll(point, 120.0);
    let zoomed_in = harness.node_map().zoom();
    assert!(zoomed_in > before, "{zoomed_in} is closer than {before}");

    harness.scroll(point, -240.0);
    assert!(harness.node_map().zoom() < zoomed_in);
}

// -------------------------------------------------------------- focus mode

#[test]
fn double_clicking_a_node_focuses_on_it() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/shape/ui/Layout.pi");
    let everything = harness.node_map().visible_files().len();

    harness.double_click(harness.node_center(file));

    assert_eq!(harness.node_map().focused(), Some(file));
    let visible = harness.node_map().visible_files();
    assert!(
        visible.len() < everything,
        "focusing hides the unrelated nodes"
    );
    assert!(visible.contains(&file), "the focused node is still drawn");
}

#[test]
fn focusing_keeps_the_nodes_the_focused_node_connects_to() {
    let mut harness = Harness::with_project();
    let layout = harness.file_id("spec/shape/ui/Layout.pi");
    let toolbar = harness.file_id("spec/shape/ui/Toolbar.pi");
    let unrelated = harness.file_id("spec/agent/skills/Testing.pi");

    harness.double_click(harness.node_center(layout));

    let visible = harness.node_map().visible_files();
    assert!(
        visible.contains(&toolbar),
        "Layout points at Toolbar, so Toolbar stays"
    );
    assert!(
        !visible.contains(&unrelated),
        "a node with no connection to Layout disappears"
    );
}

#[test]
fn focusing_rearranges_the_nodes_that_are_left() {
    let mut harness = Harness::with_project();
    let layout = harness.file_id("spec/shape/ui/Layout.pi");
    let toolbar = harness.file_id("spec/shape/ui/Toolbar.pi");
    let before = harness.node_map().screen_rect(toolbar).unwrap().center();

    harness.double_click(harness.node_center(layout));

    let after = harness.node_map().screen_rect(toolbar).unwrap().center();
    assert!(
        (after - before).length() > 1.0,
        "the layout was rebuilt around the focused node"
    );

    let focused = harness.node_map().screen_rect(layout).unwrap();
    assert!(
        focused.center().x < after.x,
        "Layout points at Toolbar, so Toolbar stands to the right of it"
    );
}

#[test]
fn escape_leaves_focus_and_brings_every_node_back() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/shape/ui/Layout.pi");
    let everything = harness.node_map().visible_files().len();

    harness.double_click(harness.node_center(file));
    assert!(harness.node_map().focused().is_some());

    harness.press_key(Key::Escape);

    assert_eq!(harness.node_map().focused(), None);
    assert_eq!(harness.node_map().visible_files().len(), everything);
}

#[test]
fn double_clicking_the_background_leaves_focus() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/shape/ui/Layout.pi");

    harness.double_click(harness.node_center(file));
    assert!(harness.node_map().focused().is_some());

    let empty = harness.empty_canvas_point();
    harness.double_click(empty);
    assert_eq!(harness.node_map().focused(), None);
}

#[test]
fn nodes_can_still_be_dragged_while_focused() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/shape/ui/Layout.pi");

    harness.double_click(harness.node_center(file));
    let before = harness.node_map().screen_rect(file).unwrap().center();
    harness.drag(before, before + vec2(60.0, -40.0));

    let after = harness.node_map().screen_rect(file).unwrap().center();
    let moved = after - before;
    assert!(
        (moved.x - 60.0).abs() < 2.0 && (moved.y + 40.0).abs() < 2.0,
        "the focused node followed the pointer, moved by {moved:?}"
    );
}

#[test]
fn focusing_another_node_from_inside_focus_rebuilds_the_map() {
    let mut harness = Harness::with_project();
    let layout = harness.file_id("spec/shape/ui/Layout.pi");
    let toolbar = harness.file_id("spec/shape/ui/Toolbar.pi");

    harness.double_click(harness.node_center(layout));
    harness.double_click(harness.node_center(toolbar));

    assert_eq!(harness.node_map().focused(), Some(toolbar));
    assert!(harness.node_map().visible_files().contains(&layout));
}

#[test]
fn opening_a_directory_clears_the_focus() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/shape/ui/Layout.pi");
    harness.double_click(harness.node_center(file));
    assert!(harness.node_map().focused().is_some());

    harness.app_mut().open_path(&project_root());
    harness.frames(2);

    assert_eq!(harness.node_map().focused(), None);
}

// ------------------------------------------------------- the canvas toolbar

/// The canvas toolbar sits in the top left corner of the canvas: a frame 10
/// points in from the corner with a 3 point margin, holding icon buttons 26
/// points tall, 2 points apart, with 9 point separators between the groups.
fn canvas_button(harness: &Harness, button: &str) -> eframe::egui::Pos2 {
    let y = match button {
        "zoom_in" => 26.0,
        "zoom_out" => 54.0,
        "fit" => 93.0,
        "focus" => 132.0,
        other => panic!("no {other} button"),
    };
    harness.node_map().viewport().min + vec2(30.0, y)
}

#[test]
fn the_canvas_toolbar_zooms_in_and_out_and_fits() {
    let mut harness = Harness::with_project();
    let fitted = harness.node_map().zoom();

    harness.click(canvas_button(&harness, "zoom_in"));
    let zoomed_in = harness.node_map().zoom();
    assert!(zoomed_in > fitted, "{zoomed_in} is closer than {fitted}");

    harness.click(canvas_button(&harness, "zoom_out"));
    assert!(harness.node_map().zoom() < zoomed_in);

    harness.click(canvas_button(&harness, "zoom_in"));
    harness.click(canvas_button(&harness, "fit"));
    assert!(
        (harness.node_map().zoom() - fitted).abs() < 0.001,
        "fitting frames the whole map again"
    );
}

#[test]
fn the_canvas_toolbar_focuses_on_the_selected_node_and_lets_it_go() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/shape/ui/Layout.pi");

    // egui holds a tooltip back until the pointer has rested for a while.
    harness.move_pointer(canvas_button(&harness, "focus"));
    harness.frames(90);
    assert!(
        harness.has_text("Focus on the selected node"),
        "the button says what it does before anything is selected"
    );

    harness.click(harness.node_center(file));
    harness.click(canvas_button(&harness, "focus"));
    assert_eq!(harness.node_map().focused(), Some(file));

    harness.click(canvas_button(&harness, "focus"));
    assert_eq!(harness.node_map().focused(), None);
}

// ------------------------------------------------------- the editor sidebar

/// The close button sits at the right end of the editor's header: a 26 point
/// tall icon button, 34 points wide, inside a 4 point margin.
fn close_button(harness: &Harness) -> eframe::egui::Pos2 {
    let sidebar = harness.sidebar();
    sidebar.right_top() + vec2(-21.0, 17.0)
}

#[test]
fn selecting_a_node_opens_the_sidebar_on_its_file() {
    let mut harness = Harness::with_project();
    assert!(
        harness.app().editor().is_none(),
        "nothing is selected, so there is no sidebar"
    );

    let file = harness.file_id("spec/shape/ui/Layout.pi");
    harness.click(harness.node_center(file));

    let editor = harness.app().editor().expect("the sidebar came out");
    assert_eq!(editor.file(), file);
    assert_eq!(editor.rel(), "spec/shape/ui/Layout.pi");
    assert_eq!(
        editor.path(),
        harness.app().specbase().unwrap().file(file).path,
        "the editor is on the file the node stands for"
    );
    assert_eq!(
        editor.text(),
        std::fs::read_to_string(editor.path()).unwrap(),
        "the editor holds what is in the file"
    );
}

#[test]
fn letting_the_selection_go_takes_the_sidebar_away() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/shape/ui/Layout.pi");

    harness.click(harness.node_center(file));
    assert!(harness.app().editor().is_some());

    let empty = harness.empty_canvas_point();
    harness.click(empty);

    assert_eq!(harness.node_map().selected(), None);
    assert!(harness.app().editor().is_none(), "the sidebar went away");
}

#[test]
fn selecting_another_node_swaps_the_file_in_the_sidebar() {
    let mut harness = Harness::with_project();
    let layout = harness.file_id("spec/shape/ui/Layout.pi");
    let toolbar = harness.file_id("spec/shape/ui/Toolbar.pi");

    harness.click(harness.node_center(layout));
    assert_eq!(harness.app().editor().unwrap().file(), layout);

    harness.click(harness.node_center(toolbar));
    assert_eq!(harness.app().editor().unwrap().file(), toolbar);
}

#[test]
fn focusing_on_a_node_brings_the_sidebar_out_for_it() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/shape/ui/Layout.pi");

    harness.double_click(harness.node_center(file));

    assert_eq!(harness.node_map().focused(), Some(file));
    assert_eq!(
        harness.app().editor().map(|editor| editor.file()),
        Some(file),
        "a focused node is a selected node, so the sidebar is out"
    );
}

#[test]
fn the_close_button_closes_the_sidebar_and_lets_the_node_go() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/shape/ui/Layout.pi");

    harness.click(harness.node_center(file));
    assert!(harness.app().editor().is_some());

    harness.click(close_button(&harness));

    assert!(harness.app().editor().is_none(), "the sidebar closed");
    assert_eq!(
        harness.node_map().selected(),
        None,
        "the node it was open on is no longer selected"
    );
}

#[test]
fn the_sidebar_takes_its_room_from_the_canvas() {
    let mut harness = Harness::with_project();
    let whole_body = harness.node_map().viewport();

    let file = harness.file_id("spec/shape/ui/Layout.pi");
    harness.click(harness.node_center(file));
    let beside_the_sidebar = harness.node_map().viewport();

    assert!(
        (whole_body.right() - beside_the_sidebar.right() - theme::SIDEBAR_WIDTH).abs() < 1.0,
        "the canvas gave the sidebar its width, and kept the rest"
    );
    assert_eq!(
        beside_the_sidebar.left(),
        whole_body.left(),
        "the canvas kept its left edge, so the map did not move under the pointer"
    );

    harness.click(close_button(&harness));
    assert_eq!(
        harness.node_map().viewport().right(),
        whole_body.right(),
        "closing the sidebar gives the room back"
    );
}

#[test]
fn the_edge_between_the_canvas_and_the_sidebar_can_be_dragged() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/shape/ui/Layout.pi");
    harness.click(harness.node_center(file));
    let before = harness.sidebar().width();

    let edge = harness.sidebar_edge();
    harness.drag(edge, edge - vec2(120.0, 0.0));

    let after = harness.sidebar();
    assert!(
        (after.width() - before - 120.0).abs() < 2.0,
        "the sidebar followed the pointer: {before} became {}",
        after.width()
    );
    assert_eq!(
        harness.node_map().viewport().right(),
        after.left(),
        "the canvas took what the sidebar gave back"
    );
    assert!(
        harness.app().editor().is_some(),
        "dragging the edge is not a click on the canvas"
    );

    let edge = harness.sidebar_edge();
    harness.drag(edge, edge + vec2(60.0, 0.0));
    assert!(
        (harness.sidebar().width() - (before + 60.0)).abs() < 2.0,
        "and it goes back the other way"
    );
}

#[test]
fn the_sidebar_stays_between_its_bounds() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/shape/ui/Layout.pi");
    harness.click(harness.node_center(file));

    let edge = harness.sidebar_edge();
    harness.drag(edge, edge + vec2(600.0, 0.0));
    assert!(
        (harness.sidebar().width() - theme::SIDEBAR_MIN_WIDTH).abs() < 2.0,
        "the sidebar stops at its narrowest, at {}",
        harness.sidebar().width()
    );

    let edge = harness.sidebar_edge();
    harness.drag(edge, edge - vec2(900.0, 0.0));
    assert!(
        (harness.sidebar().width() - theme::SIDEBAR_MAX_WIDTH).abs() < 2.0,
        "and at its widest, at {}",
        harness.sidebar().width()
    );
}

#[test]
fn the_width_the_sidebar_was_given_outlasts_the_file_in_it() {
    let mut harness = Harness::with_project();
    let layout = harness.file_id("spec/shape/ui/Layout.pi");
    let toolbar = harness.file_id("spec/shape/ui/Toolbar.pi");

    harness.click(harness.node_center(layout));
    let edge = harness.sidebar_edge();
    // Narrower, so the node picked up next is still beside the sidebar rather
    // than behind it.
    harness.drag(edge, edge + vec2(100.0, 0.0));
    let dragged_to = harness.sidebar().width();

    harness.click(harness.node_center(toolbar));
    assert_eq!(
        harness.app().editor().unwrap().file(),
        toolbar,
        "another file is in the sidebar"
    );
    assert!(
        (harness.sidebar().width() - dragged_to).abs() < 2.0,
        "the sidebar is still the width it was dragged to"
    );

    harness.click(close_button(&harness));
    harness.click(harness.node_center(layout));
    assert!(
        (harness.sidebar().width() - dragged_to).abs() < 2.0,
        "and it comes back that width"
    );
}

#[test]
fn typing_in_the_editor_leaves_the_file_alone_until_it_is_saved() {
    let project = scratch_project();
    let path = project.join("spec/Thing.pi");
    let on_disk = std::fs::read_to_string(&path).unwrap();

    let mut harness = Harness::new();
    harness.open(&project);
    let file = harness.file_id("spec/Thing.pi");
    harness.click(harness.node_center(file));

    harness.click(harness.editor_text_point());
    harness.type_text("# a note\n");

    let editor = harness.app().editor().unwrap();
    assert!(editor.text().contains("# a note"), "the typing arrived");
    assert!(editor.is_dirty());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        on_disk,
        "nothing was written yet"
    );

    let save = harness.text_position("Save").expect("the header offers Save");
    harness.click(save + vec2(10.0, 6.0));

    let editor = harness.app().editor().unwrap();
    assert!(!editor.is_dirty(), "the edits are on disk now");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), editor.text());
    assert!(editor.text().contains("# a note"));
}

#[test]
fn cancelling_puts_the_file_back_and_leaves_the_sidebar_open() {
    let project = scratch_project();
    let path = project.join("spec/Thing.pi");
    let on_disk = std::fs::read_to_string(&path).unwrap();

    let mut harness = Harness::new();
    harness.open(&project);
    let file = harness.file_id("spec/Thing.pi");
    harness.click(harness.node_center(file));

    harness.click(harness.editor_text_point());
    harness.type_text("scribble");
    assert!(harness.app().editor().unwrap().is_dirty());

    let cancel = harness
        .text_position("Cancel")
        .expect("the header offers Cancel");
    harness.click(cancel + vec2(10.0, 6.0));

    let editor = harness.app().editor().expect("the sidebar stayed open");
    assert_eq!(editor.text(), on_disk, "the buffer went back to the file");
    assert!(!editor.is_dirty());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), on_disk);
}

#[test]
fn opening_another_directory_closes_the_sidebar() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/shape/ui/Layout.pi");
    harness.click(harness.node_center(file));
    assert!(harness.app().editor().is_some());

    harness.open(&scratch_project());

    assert!(
        harness.app().editor().is_none(),
        "the editor belonged to the specbase that was open"
    );
}

// ------------------------------------------------------- the language server

/// A harness on a scratch project, with a scripted server behind the editor.
fn with_server() -> (Harness, crate::ui::editor::lsp::fake::Server, std::path::PathBuf) {
    let project = scratch_project();
    let mut harness = Harness::new();
    harness.open(&project);
    let server = harness.language_server();
    (harness, server, project)
}

#[test]
fn opening_a_file_in_the_sidebar_hands_it_to_the_server() {
    let (mut harness, server, project) = with_server();
    let file = harness.file_id("spec/Thing.pi");

    harness.click(harness.node_center(file));

    let opened = server.sent_by("textDocument/didOpen");
    assert_eq!(opened.len(), 1, "the file went to the server once");
    let document = &opened[0]["params"]["textDocument"];
    assert_eq!(
        document["uri"],
        crate::ui::editor::lsp::uri(&project.join("spec/Thing.pi").canonicalize().unwrap())
    );
    assert_eq!(
        document["text"],
        std::fs::read_to_string(project.join("spec/Thing.pi")).unwrap(),
        "the server was given what the editor is showing"
    );
}

#[test]
fn typing_tells_the_server_what_the_buffer_holds_now() {
    let (mut harness, server, _project) = with_server();
    let file = harness.file_id("spec/Thing.pi");
    harness.click(harness.node_center(file));

    harness.click(harness.editor_text_point());
    harness.type_text("x");

    let changed = server.sent_by("textDocument/didChange");
    assert!(!changed.is_empty(), "the edit went out");
    let text = changed
        .last()
        .unwrap()["params"]["contentChanges"][0]["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        text,
        harness.app().editor().unwrap().text(),
        "the server holds the same text the editor does"
    );
}

#[test]
fn saving_tells_the_server_the_file_was_written() {
    let (mut harness, server, _project) = with_server();
    let file = harness.file_id("spec/Thing.pi");
    harness.click(harness.node_center(file));
    harness.click(harness.editor_text_point());
    harness.type_text("// saved\n");

    let save = harness.text_position("Save").expect("the header offers Save");
    harness.click(save + vec2(10.0, 6.0));

    let saved = server.sent_by("textDocument/didSave");
    assert_eq!(saved.len(), 1, "the server was told the file is on disk");
}

#[test]
fn closing_the_sidebar_tells_the_server_the_file_is_closed() {
    let (mut harness, server, _project) = with_server();
    let file = harness.file_id("spec/Thing.pi");
    harness.click(harness.node_center(file));
    assert_eq!(server.sent_by("textDocument/didOpen").len(), 1);

    harness.click(close_button(&harness));

    assert_eq!(
        server.sent_by("textDocument/didClose").len(),
        1,
        "the file the editor let go of is closed"
    );
}

#[test]
fn selecting_another_node_moves_the_server_on_to_that_file() {
    let (mut harness, server, _project) = with_server();
    let thing = harness.file_id("spec/Thing.pi");
    let index = harness.file_id("spec/index.pi");

    harness.click(harness.node_center(thing));
    harness.click(harness.node_center(index));

    assert_eq!(
        server.sent_by("textDocument/didClose").len(),
        1,
        "the file that was open was closed"
    );
    let opened = server.sent_by("textDocument/didOpen");
    assert_eq!(opened.len(), 2, "and the new one was opened");
    assert_eq!(opened[1]["params"]["textDocument"]["text"], "from ./Thing export *\n");
}

#[test]
fn resting_on_a_word_asks_the_server_about_it_and_shows_the_answer() {
    let (mut harness, server, _project) = with_server();
    let file = harness.file_id("spec/Thing.pi");
    harness.click(harness.node_center(file));
    server.set_hover("```piton\nexport anchor Thing:\n```\n\n**Properties**\n\n- `pitch`\n");

    // egui only calls a rest a rest once the pointer has been still a while.
    harness.move_pointer(harness.editor_text_point());
    harness.frames(90);

    let asked = server.sent_by("textDocument/hover");
    assert_eq!(asked.len(), 1, "the word under the pointer was asked about");
    assert!(
        harness.has_text("export anchor Thing:"),
        "what the server said is on screen"
    );
    assert!(
        !harness.has_text("```"),
        "the answer is shown as words, not as markdown"
    );
}

#[test]
fn a_pointer_that_keeps_moving_is_not_asking_anything() {
    let (mut harness, server, _project) = with_server();
    let file = harness.file_id("spec/Thing.pi");
    harness.click(harness.node_center(file));
    server.set_hover("something");

    let start = harness.editor_text_point();
    for step in 0..8 {
        harness.move_pointer(start + vec2(step as f32 * 3.0, 0.0));
    }

    assert!(
        server.sent_by("textDocument/hover").is_empty(),
        "nothing was asked while the pointer was on its way somewhere"
    );
}

#[test]
fn a_problem_arriving_does_not_interrupt_the_typing_that_caused_it() {
    let (mut harness, server, _project) = with_server();
    let file = harness.file_id("spec/Thing.pi");
    harness.click(harness.node_center(file));
    let path = harness.app().specbase().unwrap().file(file).path.clone();

    harness.click(harness.editor_text_point());
    harness.type_text("one");

    // The server catches up mid-word and reports what is wrong so far.
    server.publish(
        &path,
        serde_json::json!([{
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 3 },
            },
            "severity": 1,
            "message": "unexpected `one`",
        }]),
    );
    harness.frames(2);
    assert!(harness.has_text("unexpected `one`"), "the problem is listed");

    harness.type_text("two");

    assert!(
        harness.app().editor().unwrap().text().contains("onetwo"),
        "the typing carried on: {:?}",
        harness.app().editor().unwrap().text()
    );
}

#[test]
fn a_problem_going_away_does_not_interrupt_the_typing_either() {
    let (mut harness, server, _project) = with_server();
    let file = harness.file_id("spec/Thing.pi");
    harness.click(harness.node_center(file));
    let path = harness.app().specbase().unwrap().file(file).path.clone();
    server.publish(
        &path,
        serde_json::json!([{
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 6 },
            },
            "severity": 2,
            "message": "a passing complaint",
        }]),
    );
    harness.frames(2);

    harness.click(harness.editor_text_point());
    harness.type_text("one");

    server.publish(&path, serde_json::json!([]));
    harness.frames(2);
    harness.type_text("two");

    assert!(
        harness.app().editor().unwrap().text().contains("onetwo"),
        "the typing carried on: {:?}",
        harness.app().editor().unwrap().text()
    );
}

#[test]
fn a_server_that_stops_leaves_the_editor_working() {
    let (mut harness, server, _project) = with_server();
    let file = harness.file_id("spec/Thing.pi");
    harness.click(harness.node_center(file));

    server.stop();
    harness.frames(2);

    assert!(harness.has_text("no language server"));
    // The file is still there to read and to edit.
    assert!(harness.has_text("export anchor Thing:"));
    harness.click(harness.editor_text_point());
    harness.type_text("still typing");
    assert!(
        harness.app().editor().unwrap().text().contains("still typing"),
        "the editor works on its own"
    );
}
