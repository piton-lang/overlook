//! Visual tests: what the application puts on screen.
//!
//! These read the paint list of a frame, so they check what was drawn and where
//! it was drawn, rather than what a click does.

use eframe::egui::Color32;

use super::harness::{project_root, scratch_project, Harness};
use crate::ui::theme;

#[test]
fn the_header_shows_its_two_groups_of_buttons() {
    let harness = Harness::new();
    for label in ["Open Directory", "About", "Exit"] {
        assert!(harness.has_text(label), "the header shows {label}");
    }

    let texts = harness.texts();
    let x = |label: &str| {
        texts
            .iter()
            .find(|(text, _)| text == label)
            .map(|(_, pos)| pos.x)
            .unwrap()
    };
    assert!(
        x("Open Directory") < x("About") && x("About") < x("Exit"),
        "the buttons are in the order the layout asks for"
    );
}

#[test]
fn an_empty_canvas_says_what_to_open() {
    let harness = Harness::new();
    assert!(harness.has_text("No specification open"));
    assert!(harness.has_text("piton.config.pi"));
}

#[test]
fn the_status_bar_shows_the_absolute_path_at_the_far_left() {
    let harness = Harness::with_project();
    let root = project_root().canonicalize().unwrap();
    let path = harness
        .text_containing(&root.display().to_string())
        .expect("the status bar shows the open directory");

    let (_, pos) = harness
        .texts()
        .into_iter()
        .find(|(text, _)| *text == path)
        .unwrap();
    let footer = harness.screen().bottom() - theme::FOOTER_HEIGHT;
    assert!(pos.y > footer, "the path is drawn in the footer");
    assert!(
        pos.x < 40.0,
        "the path is the far left group, drawn at {}",
        pos.x
    );
    assert!(
        std::path::Path::new(&path).is_absolute(),
        "{path} is absolute"
    );
}

#[test]
fn the_status_bar_says_when_no_directory_is_open() {
    let harness = Harness::new();
    assert!(harness.has_text("No directory open"));
}

#[test]
fn nodes_are_labelled_with_their_file_names() {
    let harness = Harness::with_project();
    for name in ["index.pi", "Layout.pi", "piton.config.pi"] {
        assert!(harness.has_text(name), "the canvas labels {name}");
    }
}

#[test]
fn the_entry_and_config_files_are_badged() {
    let harness = Harness::with_project();
    assert!(harness.has_text("entry"));
    assert!(harness.has_text("config"));
}

#[test]
fn focusing_names_the_focused_node_in_the_status_bar() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/shape/ui/Layout.pi");
    harness.double_click(harness.node_center(file));

    assert!(
        harness.has_text("Focused on Layout.pi"),
        "the status bar reports the focus"
    );
}

#[test]
fn focusing_paints_a_quieter_canvas() {
    let mut harness = Harness::with_project();
    let busy = harness.shape_count();

    let file = harness.file_id("spec/shape/ui/Layout.pi");
    harness.double_click(harness.node_center(file));

    assert!(
        harness.shape_count() < busy,
        "a focused canvas draws less than the whole map"
    );
}

#[test]
fn the_body_is_painted_with_the_canvas_colour() {
    let harness = Harness::with_project();
    let screen = harness.screen();
    let canvas = harness
        .rects()
        .into_iter()
        .find(|painted| painted.fill == theme::CANVAS && painted.rect.width() >= screen.width())
        .expect("the canvas fills the body");

    assert!(
        canvas.rect.top() >= theme::HEADER_HEIGHT - 1.0,
        "the canvas starts below the header"
    );
    assert!(
        canvas.rect.bottom() <= screen.bottom() - theme::FOOTER_HEIGHT + 1.0,
        "the canvas stops above the footer"
    );
}

#[test]
fn a_selected_node_is_outlined_in_the_accent_colour() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/shape/ui/Layout.pi");
    let outlined = |harness: &Harness, node: eframe::egui::Rect| {
        harness.rects().into_iter().any(|painted| {
            painted.stroke == theme::ACCENT
                && (painted.rect.center() - node.center()).length() < 2.0
        })
    };

    let node = harness.node_map().screen_rect(file).unwrap();
    assert!(!outlined(&harness, node), "an unselected node is not accented");

    harness.click(node.center());
    let node = harness.node_map().screen_rect(file).unwrap();
    assert!(outlined(&harness, node), "the selected node is accented");
}

#[test]
fn text_is_readable_against_the_surface_it_is_drawn_on() {
    // Body text has to clear 4.5:1, dimmer supporting text 3:1.
    assert!(contrast(theme::TEXT, theme::PANEL) >= 4.5);
    assert!(contrast(theme::TEXT, theme::CANVAS) >= 4.5);
    assert!(contrast(theme::TEXT, theme::NODE_FILL) >= 4.5);
    assert!(contrast(theme::TEXT, theme::NODE_HEADER) >= 4.5);

    assert!(contrast(theme::TEXT_DIM, theme::PANEL) >= 3.0);
    assert!(contrast(theme::TEXT_FAINT, theme::CANVAS) >= 2.0);

    assert!(contrast(theme::ACCENT, theme::PANEL) >= 4.5);
    assert!(contrast(theme::WARN, theme::NODE_FILL) >= 4.5);
    assert!(contrast(theme::DANGER, theme::PANEL) >= 4.5);
}

#[test]
fn declaration_colours_stay_apart_from_each_other() {
    let kinds = ["anchor", "skill", "command", "build-skill", "piton-config"];
    for kind in kinds {
        assert!(
            contrast(theme::kind_color(kind), theme::NODE_FILL) >= 4.5,
            "{kind} is readable on a node"
        );
        // The same keyword always gets the same colour.
        assert_eq!(theme::kind_color(kind), theme::kind_color(kind));
    }
}

// ------------------------------------------------------- the editor sidebar

#[test]
fn nothing_is_selected_so_the_canvas_has_the_body_to_itself() {
    let harness = Harness::with_project();
    assert!(
        !harness.has_text("Save") && !harness.has_text("Cancel"),
        "no sidebar is drawn until a node is selected"
    );
    assert_eq!(
        harness.node_map().viewport().right(),
        harness.screen().right(),
        "the canvas reaches the right edge of the window"
    );
}

#[test]
fn the_sidebar_draws_the_editor_beside_the_canvas() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/shape/ui/Layout.pi");
    harness.click(harness.node_center(file));

    let sidebar = harness.sidebar();
    for label in ["Save", "Cancel"] {
        let pos = harness
            .text_position(label)
            .unwrap_or_else(|| panic!("the editor header shows {label}"));
        assert!(
            pos.x > sidebar.left(),
            "{label} is drawn in the sidebar, at {}",
            pos.x
        );
        assert!(
            pos.y < sidebar.top() + 40.0,
            "{label} is drawn in the header on top, at {}",
            pos.y
        );
    }

    let name = harness
        .text_position("spec/shape/ui/Layout.pi")
        .expect("the sidebar names the file it is on");
    assert!(name.x > sidebar.left(), "the name is drawn in the sidebar");

    let surface = harness
        .rects()
        .into_iter()
        .find(|painted| painted.fill == theme::BACKDROP && sidebar.contains(painted.rect.center()))
        .expect("the text of the file is drawn on a surface of its own");
    assert!(
        surface.rect.top() > sidebar.top(),
        "the surface starts under the header"
    );
}

#[test]
fn the_sidebar_shows_the_text_of_the_file() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/shape/Stack.pi");
    harness.click(harness.node_center(file));

    assert!(
        harness.has_text("export anchor Stack:"),
        "the file is on screen, not just its name"
    );
}

#[test]
fn unsaved_edits_are_marked_in_the_sidebar() {
    let mut harness = Harness::new();
    harness.open(&scratch_project());
    let file = harness.file_id("spec/Thing.pi");
    harness.click(harness.node_center(file));

    assert!(
        !harness.has_text("unsaved"),
        "a freshly opened file has nothing to save"
    );

    harness.click(harness.editor_text_point());
    harness.type_text("edited");

    let marker = harness
        .text_position("unsaved")
        .expect("the sidebar marks the unsaved edits");
    assert!(marker.x > harness.sidebar().left());
}

#[test]
fn editor_text_is_readable_on_the_surface_it_is_drawn_on() {
    // The file itself, on the editor's own surface.
    assert!(contrast(theme::TEXT, theme::BACKDROP) >= 4.5);
    // The name of the file, and the mark on it while it has unsaved edits.
    assert!(contrast(theme::TEXT_DIM, theme::PANEL) >= 3.0);
    assert!(contrast(theme::WARN, theme::PANEL) >= 3.0);
}

/// The WCAG contrast ratio between two opaque colours.
fn contrast(a: Color32, b: Color32) -> f32 {
    let (light, dark) = {
        let (la, lb) = (luminance(a), luminance(b));
        if la > lb {
            (la, lb)
        } else {
            (lb, la)
        }
    };
    (light + 0.05) / (dark + 0.05)
}

fn luminance(color: Color32) -> f32 {
    let channel = |value: u8| {
        let v = value as f32 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
}

// ------------------------------------------------------------ highlighting

#[test]
fn the_file_is_coloured_by_the_grammar() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/shape/Stack.pi");
    harness.click(harness.node_center(file));

    // spec/shape/Stack.pi reads:
    //     export anchor Stack:
    //         language: Rust
    for keyword in ["export", "anchor"] {
        assert_eq!(
            harness.color_of(keyword),
            Some(theme::SYNTAX_KEYWORD),
            "{keyword} is drawn as the keyword it is"
        );
    }
    assert_eq!(
        harness.color_of("Stack"),
        Some(theme::SYNTAX_DECLARATION),
        "the name the file declares stands out from the keywords"
    );
    assert_eq!(harness.color_of("language"), Some(theme::SYNTAX_PROPERTY));
    assert_eq!(harness.color_of("Rust"), Some(theme::SYNTAX_TEXT));
}

#[test]
fn every_class_the_grammar_colours_is_readable_on_the_editor_surface() {
    use crate::ui::editor::highlight::Class;

    for class in [
        Class::Keyword,
        Class::Type,
        Class::Declaration,
        Class::Property,
        Class::Name,
        Class::Text,
        Class::Number,
        Class::Comment,
        Class::Path,
        Class::Custom,
        Class::Literal,
        Class::Sigil,
        Class::Plain,
    ] {
        let ratio = contrast(class.color(), theme::BACKDROP);
        let floor = match class {
            // Punctuation and comments are supporting text, and are allowed to
            // sit back; everything else is read word by word.
            Class::Comment | Class::Operator => 3.0,
            _ => 4.5,
        };
        assert!(
            ratio >= floor,
            "{class:?} is {ratio:.1}:1 on the editor surface, needs {floor}:1"
        );
    }
    assert!(contrast(theme::SYNTAX_OPERATOR, theme::BACKDROP) >= 3.0);
}

#[test]
fn the_classes_are_told_apart_from_each_other() {
    use crate::ui::editor::highlight::Class;

    // Two classes that look the same are one class as far as a reader is
    // concerned, so the ones that sit next to each other have to differ.
    let neighbours = [
        (Class::Keyword, Class::Declaration),
        (Class::Declaration, Class::Property),
        (Class::Property, Class::Text),
        (Class::Custom, Class::Declaration),
        (Class::Path, Class::Keyword),
        (Class::Type, Class::Name),
    ];
    for (one, other) in neighbours {
        assert_ne!(
            one.color(),
            other.color(),
            "{one:?} and {other:?} are drawn side by side"
        );
    }
}

// -------------------------------------------------------- language server

#[test]
fn a_problem_is_listed_and_underlined_where_it_falls() {
    let mut harness = Harness::new();
    harness.open(&scratch_project());
    let server = harness.language_server();
    let file = harness.file_id("spec/Thing.pi");
    harness.click(harness.node_center(file));

    let path = harness.app().specbase().unwrap().file(file).path.clone();
    server.publish(
        &path,
        serde_json::json!([{
            // `Thing` on the first line of the scratch project's file.
            "range": {
                "start": { "line": 0, "character": 14 },
                "end": { "line": 0, "character": 19 },
            },
            "severity": 1,
            "message": "cannot find `Thing` in this scope",
        }]),
    );
    harness.frames(2);

    let listed = harness
        .text_position("cannot find `Thing` in this scope")
        .expect("the problem is listed");
    let sidebar = harness.sidebar();
    assert!(listed.x > sidebar.left(), "it is listed in the sidebar");
    assert!(
        listed.y > sidebar.center().y,
        "it is listed at the foot of it, under the file"
    );
    let underlined: Vec<_> = harness
        .painted_text()
        .into_iter()
        .filter(|painted| painted.underline == theme::DANGER)
        .collect();
    assert!(
        underlined.iter().any(|painted| painted.text.contains("Thing")),
        "the text the problem is about is underlined, not the rest: {:?}",
        underlined.iter().map(|p| &p.text).collect::<Vec<_>>()
    );
}

#[test]
fn a_warning_is_marked_apart_from_an_error() {
    let mut harness = Harness::new();
    harness.open(&scratch_project());
    let server = harness.language_server();
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
            "message": "this export is never reached",
        }]),
    );
    harness.frames(2);

    assert_eq!(
        harness
            .painted_text()
            .into_iter()
            .find(|painted| painted.text == "this export is never reached")
            .map(|painted| painted.color),
        Some(theme::WARN),
        "a warning is not drawn as an error"
    );
}

#[test]
fn the_sidebar_says_when_there_is_no_language_server() {
    let mut harness = Harness::with_project();
    let file = harness.file_id("spec/shape/Stack.pi");
    harness.click(harness.node_center(file));

    assert!(
        harness.has_text("no language server"),
        "the editor says the file is on its own"
    );

    harness.language_server();
    harness.frames(2);
    assert!(
        !harness.has_text("no language server"),
        "a server that answers is not announced"
    );
}
