//! One place for the colours and metrics the whole application shares.

use eframe::egui::{self, Color32, CornerRadius, Stroke};

pub const BACKDROP: Color32 = Color32::from_rgb(0x0d, 0x0f, 0x14);
pub const CANVAS: Color32 = Color32::from_rgb(0x11, 0x14, 0x1b);
pub const CANVAS_GRID: Color32 = Color32::from_rgb(0x1b, 0x1f, 0x29);
pub const PANEL: Color32 = Color32::from_rgb(0x16, 0x19, 0x21);
pub const BORDER: Color32 = Color32::from_rgb(0x25, 0x2a, 0x36);

pub const TEXT: Color32 = Color32::from_rgb(0xd6, 0xdb, 0xe6);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x79, 0x82, 0x96);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x51, 0x59, 0x6b);

pub const ACCENT: Color32 = Color32::from_rgb(0x6e, 0xa8, 0xff);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(0x2b, 0x3c, 0x5e);
pub const WARN: Color32 = Color32::from_rgb(0xd8, 0x9a, 0x4c);
pub const DANGER: Color32 = Color32::from_rgb(0xe0, 0x6c, 0x6c);

pub const NODE_FILL: Color32 = Color32::from_rgb(0x1a, 0x1f, 0x2a);
pub const NODE_HEADER: Color32 = Color32::from_rgb(0x22, 0x28, 0x36);
pub const NODE_BORDER: Color32 = Color32::from_rgb(0x30, 0x38, 0x49);
pub const GROUP_FILL: Color32 = Color32::from_rgb(0x14, 0x17, 0x1f);
pub const GROUP_BORDER: Color32 = Color32::from_rgb(0x24, 0x2a, 0x37);

pub const EDGE_REFERENCE: Color32 = Color32::from_rgb(0x4d, 0x6f, 0xa8);
pub const EDGE_IMPORT: Color32 = Color32::from_rgb(0x38, 0x42, 0x55);
pub const EDGE_USE: Color32 = Color32::from_rgb(0x6b, 0x54, 0x8f);
pub const EDGE_HIGHLIGHT: Color32 = Color32::from_rgb(0x8f, 0xbe, 0xff);

pub const RADIUS: CornerRadius = CornerRadius::same(6);
pub const RADIUS_SMALL: CornerRadius = CornerRadius::same(4);

pub const HEADER_HEIGHT: f32 = 38.0;
pub const FOOTER_HEIGHT: f32 = 24.0;

// The syntax palette, in the classes the Piton grammar puts tokens in. They
// are drawn on BACKDROP, the editor's surface.
pub const SYNTAX_KEYWORD: Color32 = Color32::from_rgb(0xc9, 0x9d, 0xf0);
pub const SYNTAX_DECLARATION: Color32 = Color32::from_rgb(0x7f, 0xb3, 0xff);
pub const SYNTAX_CUSTOM: Color32 = Color32::from_rgb(0x8e, 0xd6, 0xa8);
pub const SYNTAX_TYPE: Color32 = Color32::from_rgb(0x76, 0xd2, 0xd8);
pub const SYNTAX_PROPERTY: Color32 = Color32::from_rgb(0xe0, 0xb0, 0x76);
pub const SYNTAX_NAME: Color32 = TEXT;
pub const SYNTAX_TEXT: Color32 = Color32::from_rgb(0x9d, 0xbf, 0x8f);
pub const SYNTAX_NUMBER: Color32 = Color32::from_rgb(0xe6, 0xd1, 0x78);
pub const SYNTAX_LITERAL: Color32 = Color32::from_rgb(0xe6, 0xd1, 0x78);
pub const SYNTAX_OPERATOR: Color32 = Color32::from_rgb(0x94, 0x9c, 0xb0);
pub const SYNTAX_COMMENT: Color32 = Color32::from_rgb(0x7a, 0x85, 0x98);
pub const SYNTAX_PATH: Color32 = Color32::from_rgb(0xf0, 0x9d, 0xb4);
pub const SYNTAX_SIGIL: Color32 = Color32::from_rgb(0xd9, 0xa5, 0xe8);

/// The sidebar the node map opens beside the canvas for the selected node.
/// It opens at this width, and can be dragged between the two bounds.
pub const SIDEBAR_WIDTH: f32 = 400.0;
pub const SIDEBAR_MIN_WIDTH: f32 = 260.0;
pub const SIDEBAR_MAX_WIDTH: f32 = 900.0;

/// A colour for a declaration keyword, stable across runs.
pub fn kind_color(kind: &str) -> Color32 {
    const PALETTE: [Color32; 6] = [
        Color32::from_rgb(0x7f, 0xb3, 0xff),
        Color32::from_rgb(0x8e, 0xd6, 0xa8),
        Color32::from_rgb(0xe0, 0xb0, 0x76),
        Color32::from_rgb(0xc9, 0x9d, 0xf0),
        Color32::from_rgb(0x76, 0xd2, 0xd8),
        Color32::from_rgb(0xf0, 0x9d, 0xb4),
    ];
    let mut hash: u32 = 2166136261;
    for byte in kind.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    PALETTE[(hash as usize) % PALETTE.len()]
}

/// Apply the application's visual style to a context.
pub fn install(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = BACKDROP;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_DIM);
    visuals.override_text_color = Some(TEXT);
    ctx.set_visuals(visuals);

    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(6.0, 6.0);
    });
}
