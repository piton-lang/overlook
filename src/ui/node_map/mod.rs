//! An infinite canvas of interconnected nodes for the folders and files of a
//! specbase, with connections drawn between connected anchors and symbols.
//!
//! Files and symbols that are never reached from the project entry point are
//! drawn too, marked so they stand out.
//!
//! Nodes can be dragged around, selected, and focused on. Focusing rearranges
//! the canvas around one node: every unrelated node disappears and what is left
//! is the node and the nodes it connects to.
//!
//! While a single node is selected the canvas opens a sidebar beside itself
//! with the editor for that node's file.

mod arrange;

use eframe::egui::{
    self, epaint::CubicBezierShape, pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Sense, Shape,
    Stroke, Ui, Vec2,
};

use crate::specbase::{EdgeKind, EndPoint, FileId, Specbase};

use super::editor::{Editor, EditorAction, Lsp};
use super::icon::Icon;
use super::theme;
use super::toolbar::{Toolbar, ToolbarButton, ToolbarGroup};

use arrange::{Arrangement, FILE_HEADER};

const MIN_ZOOM: f32 = 0.12;
const MAX_ZOOM: f32 = 2.5;
const ROWS_VISIBLE_ABOVE: f32 = 0.42;
const LABELS_VISIBLE_ABOVE: f32 = 0.22;

/// The node the canvas is focused on, and the layout built around it.
struct Focus {
    file: FileId,
    arrangement: Arrangement,
}

pub struct NodeMap {
    pan: Vec2,
    zoom: f32,
    /// The whole specbase, laid out by folder.
    full: Arrangement,
    /// Set while the canvas is focused on one node.
    focus: Option<Focus>,
    /// Per file displacement from dragging nodes around the canvas.
    offsets: Vec<Vec2>,
    selected: Option<FileId>,
    hovered: Option<FileId>,
    dragging: Option<FileId>,
    fit_pending: bool,
    /// The area the canvas was last drawn in.
    viewport: Rect,
}

impl Default for NodeMap {
    fn default() -> Self {
        NodeMap {
            pan: Vec2::ZERO,
            zoom: 1.0,
            full: Arrangement::default(),
            focus: None,
            offsets: Vec::new(),
            selected: None,
            hovered: None,
            dragging: None,
            fit_pending: false,
            viewport: Rect::from_min_size(Pos2::ZERO, vec2(1.0, 1.0)),
        }
    }
}

impl NodeMap {
    /// Lay out a newly opened specbase.
    pub fn set_specbase(&mut self, specbase: &Specbase) {
        self.full = arrange::arrange(specbase);
        self.focus = None;
        self.offsets = vec![Vec2::ZERO; specbase.files.len()];
        self.selected = None;
        self.hovered = None;
        self.dragging = None;
        self.pan = Vec2::ZERO;
        self.zoom = 1.0;
        self.fit_pending = true;
    }

    pub fn selected(&self) -> Option<FileId> {
        self.selected
    }

    /// Let the selected node go; the sidebar closes with it.
    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    pub fn hovered(&self) -> Option<FileId> {
        self.hovered
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// The node the canvas is focused on, when it is focused on one.
    pub fn focused(&self) -> Option<FileId> {
        self.focus.as_ref().map(|focus| focus.file)
    }

    // The three accessors below describe what is on the canvas. The
    // application itself does not need them; the tests read the canvas
    // through them.

    /// The files the canvas currently draws; everything else has disappeared.
    #[allow(dead_code)]
    pub fn visible_files(&self) -> Vec<FileId> {
        self.arrangement().files().collect()
    }

    /// The area the canvas was last drawn in.
    #[allow(dead_code)]
    pub fn viewport(&self) -> Rect {
        self.viewport
    }

    /// Where a node sits on screen, when the canvas is showing it.
    #[allow(dead_code)]
    pub fn screen_rect(&self, file: FileId) -> Option<Rect> {
        let world = self.node_rect(file)?;
        Some(Rect::from_min_max(
            self.to_screen(self.viewport, world.min),
            self.to_screen(self.viewport, world.max),
        ))
    }

    /// Focus the canvas on one node, or clear the focus with `None`.
    ///
    /// Focusing rearranges the canvas: the focused node and the nodes it
    /// connects to are laid out afresh, and the rest disappear.
    pub fn set_focus(&mut self, specbase: &Specbase, file: Option<FileId>) {
        self.focus = file.map(|file| Focus {
            file,
            arrangement: arrange::focus(specbase, file),
        });
        // Displacements belong to the layout they were dragged in.
        for offset in &mut self.offsets {
            *offset = Vec2::ZERO;
        }
        if let Some(file) = file {
            self.selected = Some(file);
        }
        self.dragging = None;
        self.fit_pending = true;
    }

    /// The layout currently on screen.
    fn arrangement(&self) -> &Arrangement {
        match &self.focus {
            Some(focus) => &focus.arrangement,
            None => &self.full,
        }
    }

    pub fn show(&mut self, ui: &mut Ui, specbase: Option<&Specbase>) {
        let rect = ui.available_rect_before_wrap();
        self.viewport = rect;
        ui.advance_cursor_after_rect(rect);
        // The canvas carries an id of its own rather than the positional one it
        // would be given: that id shifts the moment the sidebar opens, and a
        // shifted id drops the rest of the gesture that opened it, double
        // clicks included.
        //
        // It also stops short of the sidebar, whose edge is dragged through a
        // strip that reaches back into the canvas. The canvas is drawn after
        // the sidebar, so it would otherwise be the one that took that drag,
        // and the edge could never be moved.
        let response = ui.interact(
            self.reachable(ui, rect),
            egui::Id::new("node_map_canvas"),
            Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0, theme::CANVAS);

        let Some(specbase) = specbase else {
            self.empty_state(&painter, rect);
            return;
        };

        self.handle_input(ui, rect, &response, specbase);
        // Fitting after the input so a fresh layout is framed the same frame it
        // is built.
        if self.fit_pending {
            self.fit(rect);
            self.fit_pending = false;
        }

        self.draw_grid(&painter, rect);
        // Folder boxes sit behind the connections, the file nodes in front of
        // them, so a connection reads as running between two nodes.
        self.draw_groups(&painter, rect, specbase);
        self.draw_edges(&painter, rect, specbase);
        self.draw_nodes(&painter, rect, specbase);
        self.overlay(ui, rect, specbase);
    }

    /// The part of the canvas that answers the pointer: all of it, less the
    /// strip the sidebar's edge is dragged by when the sidebar is out.
    fn reachable(&self, ui: &Ui, rect: Rect) -> Rect {
        let beside_the_sidebar = rect.right() < ui.ctx().viewport_rect().right() - 1.0;
        if !beside_the_sidebar {
            return rect;
        }
        let grab = ui.style().interaction.resize_grab_radius_side;
        Rect::from_min_max(rect.min, pos2(rect.right() - grab, rect.bottom()))
    }

    // ---------------------------------------------------------------- view

    fn to_screen(&self, viewport: Rect, point: Pos2) -> Pos2 {
        viewport.min + point.to_vec2() * self.zoom + self.pan
    }

    fn to_world(&self, viewport: Rect, point: Pos2) -> Pos2 {
        ((point - viewport.min - self.pan) / self.zoom).to_pos2()
    }

    fn node_rect(&self, file: FileId) -> Option<Rect> {
        self.arrangement()
            .node(file)
            .map(|n| n.rect.translate(self.offsets[file]))
    }

    fn content_bounds(&self) -> Rect {
        let arrangement = self.arrangement();
        let mut bounds = arrangement.bounds;
        for node in &arrangement.nodes {
            bounds = bounds.union(node.rect.translate(self.offsets[node.file]));
        }
        bounds
    }

    fn fit(&mut self, viewport: Rect) {
        let bounds = self.content_bounds();
        if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
            return;
        }
        let margin = 32.0;
        let scale_x = (viewport.width() - margin * 2.0) / bounds.width();
        let scale_y = (viewport.height() - margin * 2.0) / bounds.height();
        self.zoom = scale_x.min(scale_y).clamp(MIN_ZOOM, 1.0);
        self.center_on(viewport, bounds.center());
    }

    fn center_on(&mut self, viewport: Rect, world: Pos2) {
        self.pan = viewport.center() - viewport.min - world.to_vec2() * self.zoom;
    }

    fn zoom_by(&mut self, viewport: Rect, factor: f32, anchor: Pos2) {
        let before = self.to_world(viewport, anchor);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        self.pan = anchor - viewport.min - before.to_vec2() * self.zoom;
    }

    // ----------------------------------------------------------- interaction

    fn handle_input(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        response: &egui::Response,
        specbase: &Specbase,
    ) {
        let pointer = response.hover_pos();

        if let Some(pointer) = pointer {
            let (scroll, zoom_delta) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let factor = zoom_delta * (scroll * 0.0015).exp();
            if (factor - 1.0).abs() > f32::EPSILON {
                self.zoom_by(viewport, factor, pointer);
            }
            self.hovered = self.hit_test(self.to_world(viewport, pointer));
        } else {
            self.hovered = None;
        }

        // Only the left button picks a node up. Any other button drags the
        // canvas, over a node as readily as anywhere else, so the middle button
        // moves the map around without ever moving what is under it.
        if response.drag_started() {
            self.dragging = response
                .drag_started_by(egui::PointerButton::Primary)
                .then(|| {
                    response
                        .interact_pointer_pos()
                        .and_then(|p| self.hit_test(self.to_world(viewport, p)))
                })
                .flatten();
        }
        if response.dragged() {
            match self.dragging {
                Some(file) => self.offsets[file] += response.drag_delta() / self.zoom,
                None => self.pan += response.drag_delta(),
            }
        }
        if response.drag_stopped() {
            self.dragging = None;
        }

        if response.clicked() {
            self.selected = response
                .interact_pointer_pos()
                .and_then(|p| self.hit_test(self.to_world(viewport, p)));
        }

        // Double clicking a node focuses on it; double clicking the background
        // leaves the focus, or frames everything when there is no focus.
        if response.double_clicked() {
            let hit = response
                .interact_pointer_pos()
                .and_then(|p| self.hit_test(self.to_world(viewport, p)));
            match hit {
                Some(file) => self.set_focus(specbase, Some(file)),
                None if self.focus.is_some() => self.set_focus(specbase, None),
                None => self.fit(viewport),
            }
        }

        if self.focus.is_some() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.set_focus(specbase, None);
        }
    }

    fn hit_test(&self, world: Pos2) -> Option<FileId> {
        self.arrangement()
            .nodes
            .iter()
            .rev()
            .find(|node| node.rect.translate(self.offsets[node.file]).contains(world))
            .map(|node| node.file)
    }

    // -------------------------------------------------------------- painting

    fn empty_state(&self, painter: &egui::Painter, rect: Rect) {
        let center = rect.center();
        Icon::Graph.paint(
            painter,
            Rect::from_center_size(center - vec2(0.0, 34.0), vec2(40.0, 40.0)),
            theme::TEXT_FAINT,
        );
        painter.text(
            center + vec2(0.0, 4.0),
            Align2::CENTER_CENTER,
            "No specification open",
            FontId::proportional(15.0),
            theme::TEXT_DIM,
        );
        painter.text(
            center + vec2(0.0, 26.0),
            Align2::CENTER_CENTER,
            format!("Open a directory containing {}", crate::specbase::CONFIG_FILE),
            FontId::proportional(12.5),
            theme::TEXT_FAINT,
        );
    }

    fn draw_grid(&self, painter: &egui::Painter, viewport: Rect) {
        let spacing = 48.0 * self.zoom;
        if spacing < 14.0 {
            return;
        }
        let origin = self.to_screen(viewport, pos2(0.0, 0.0));
        let start_x = viewport.left() - (viewport.left() - origin.x).rem_euclid(spacing);
        let start_y = viewport.top() - (viewport.top() - origin.y).rem_euclid(spacing);
        let radius = (0.9 * self.zoom).clamp(0.6, 1.4);

        let mut y = start_y;
        while y < viewport.bottom() {
            let mut x = start_x;
            while x < viewport.right() {
                painter.circle_filled(pos2(x, y), radius, theme::CANVAS_GRID);
                x += spacing;
            }
            y += spacing;
        }
    }

    fn draw_groups(&self, painter: &egui::Painter, viewport: Rect, specbase: &Specbase) {
        for group in &self.arrangement().groups {
            let rect = Rect::from_min_max(
                self.to_screen(viewport, group.rect.min),
                self.to_screen(viewport, group.rect.max),
            );
            if !rect.intersects(viewport) {
                continue;
            }
            let dir = specbase.dir(group.dir);
            let shade = 1.0 + group.depth as f32 * 0.06;
            painter.rect_filled(rect, theme::RADIUS, theme::GROUP_FILL.gamma_multiply(shade));
            painter.rect_stroke(
                rect,
                theme::RADIUS,
                Stroke::new(1.0, theme::GROUP_BORDER),
                egui::StrokeKind::Inside,
            );
            if self.zoom > LABELS_VISIBLE_ABOVE {
                let size = arrange::DIR_HEADER * 0.52 * self.zoom;
                let icon_size = size * 1.1;
                let left = rect.left() + arrange::DIR_PADDING * self.zoom;
                let baseline = rect.top() + arrange::DIR_HEADER * 0.62 * self.zoom;
                Icon::FolderOpen.paint(
                    painter,
                    Rect::from_center_size(
                        pos2(left + icon_size / 2.0, baseline),
                        vec2(icon_size, icon_size),
                    ),
                    theme::TEXT_FAINT,
                );
                painter.text(
                    pos2(left + icon_size + 6.0 * self.zoom, baseline),
                    Align2::LEFT_CENTER,
                    &dir.name,
                    FontId::proportional(size.max(1.0)),
                    theme::TEXT_DIM,
                );
            }
        }
    }

    fn draw_nodes(&self, painter: &egui::Painter, viewport: Rect, specbase: &Specbase) {
        let show_rows = self.zoom > ROWS_VISIBLE_ABOVE;
        let show_labels = self.zoom > LABELS_VISIBLE_ABOVE;

        for node in &self.arrangement().nodes {
            let world = node.rect.translate(self.offsets[node.file]);
            let rect = Rect::from_min_max(
                self.to_screen(viewport, world.min),
                self.to_screen(viewport, world.max),
            );
            if !rect.intersects(viewport) {
                continue;
            }
            let file = specbase.file(node.file);
            let selected = self.selected == Some(node.file) || self.focused() == Some(node.file);
            let hovered = self.hovered == Some(node.file);

            painter.rect_filled(rect, theme::RADIUS, theme::NODE_FILL);
            let header_height = FILE_HEADER * self.zoom;
            let header = Rect::from_min_size(rect.min, vec2(rect.width(), header_height));
            painter.rect_filled(header, theme::RADIUS, theme::NODE_HEADER);
            painter.rect_filled(
                Rect::from_min_size(
                    pos2(header.left(), header.bottom() - header_height.min(6.0)),
                    vec2(header.width(), header_height.min(6.0)),
                ),
                0,
                theme::NODE_HEADER,
            );

            let border = if selected {
                Stroke::new(1.8, theme::ACCENT)
            } else if hovered {
                Stroke::new(1.4, theme::TEXT_DIM)
            } else if !file.reached {
                Stroke::new(1.0, theme::WARN.gamma_multiply(0.7))
            } else {
                Stroke::new(1.0, theme::NODE_BORDER)
            };
            if file.reached || selected || hovered {
                painter.rect_stroke(rect, theme::RADIUS, border, egui::StrokeKind::Inside);
            } else {
                dashed_rect(painter, rect, border);
            }

            if !show_labels {
                continue;
            }

            let pad = 9.0 * self.zoom;
            let name_size = (12.5 * self.zoom).max(1.0);
            let name_color = if file.reached { theme::TEXT } else { theme::WARN };
            let mut cursor = rect.left() + pad;
            let icon_size = 12.0 * self.zoom;
            Icon::File.paint(
                painter,
                Rect::from_center_size(
                    pos2(cursor + icon_size / 2.0, header.center().y),
                    vec2(icon_size, icon_size),
                ),
                if file.reached { theme::TEXT_DIM } else { theme::WARN },
            );
            cursor += icon_size + 5.0 * self.zoom;
            painter.text(
                pos2(cursor, header.center().y),
                Align2::LEFT_CENTER,
                &file.name,
                FontId::proportional(name_size),
                name_color,
            );

            if let Some(badge) = badge_text(file) {
                painter.text(
                    pos2(rect.right() - pad, header.center().y),
                    Align2::RIGHT_CENTER,
                    badge,
                    FontId::proportional((10.0 * self.zoom).max(1.0)),
                    theme::ACCENT,
                );
            }

            if !show_rows {
                continue;
            }

            if file.symbols.is_empty() {
                painter.text(
                    pos2(rect.left() + pad, rect.top() + arrange::row_offset(0) * self.zoom),
                    Align2::LEFT_CENTER,
                    "no declarations",
                    FontId::proportional((10.5 * self.zoom).max(1.0)),
                    theme::TEXT_FAINT,
                );
            }

            for (index, symbol) in file.symbols.iter().enumerate() {
                let y = rect.top() + arrange::row_offset(index) * self.zoom;
                let color = theme::kind_color(&symbol.kind);
                let alpha = if symbol.reached { 1.0 } else { 0.45 };
                let marker = Rect::from_center_size(
                    pos2(rect.left() + pad + 3.0 * self.zoom, y),
                    vec2(6.0 * self.zoom, 6.0 * self.zoom),
                );
                painter.rect_filled(
                    marker,
                    theme::RADIUS_SMALL,
                    color.gamma_multiply(alpha),
                );

                let mut x = marker.right() + 6.0 * self.zoom;
                let kind_font = FontId::proportional((10.5 * self.zoom).max(1.0));
                let kind_text = if symbol.is_abstract {
                    format!("abstract {}", symbol.kind)
                } else {
                    symbol.kind.clone()
                };
                let kind_width = painter
                    .layout_no_wrap(kind_text.clone(), kind_font.clone(), color)
                    .size()
                    .x;
                painter.text(
                    pos2(x, y),
                    Align2::LEFT_CENTER,
                    kind_text,
                    kind_font,
                    color.gamma_multiply(alpha * 0.85),
                );
                x += kind_width + 6.0 * self.zoom;
                painter.text(
                    pos2(x, y),
                    Align2::LEFT_CENTER,
                    &symbol.name,
                    FontId::proportional((11.5 * self.zoom).max(1.0)),
                    if symbol.reached {
                        theme::TEXT
                    } else {
                        theme::TEXT_FAINT
                    },
                );
                if !symbol.reached {
                    painter.text(
                        pos2(rect.right() - pad, y),
                        Align2::RIGHT_CENTER,
                        "unreached",
                        FontId::proportional((9.5 * self.zoom).max(1.0)),
                        theme::WARN.gamma_multiply(0.8),
                    );
                }
            }
        }
    }

    fn draw_edges(&self, painter: &egui::Painter, viewport: Rect, specbase: &Specbase) {
        // Connections to and from the node under the pointer, or the selected
        // one, are drawn brighter than the rest.
        let highlight = self.selected.or(self.hovered);
        let show_rows = self.zoom > ROWS_VISIBLE_ABOVE;

        for edge in &specbase.edges {
            let Some(from) = self.endpoint(edge.from, show_rows) else { continue };
            let Some(to) = self.endpoint(edge.to, show_rows) else { continue };

            let related = highlight
                .map(|f| edge.from.file == f || edge.to.file == f)
                .unwrap_or(false);
            let base = match edge.kind {
                EdgeKind::Reference => theme::EDGE_REFERENCE,
                EdgeKind::Import => theme::EDGE_IMPORT,
                EdgeKind::Use => theme::EDGE_USE,
            };
            let color = if related {
                theme::EDGE_HIGHLIGHT
            } else if highlight.is_some() {
                base.gamma_multiply(0.35)
            } else {
                base
            };
            let width = if related { 2.0 } else { 1.2 } * self.zoom.clamp(0.5, 1.4);

            let (start, end, control) = if edge.from.file == edge.to.file {
                self_link(from, to)
            } else {
                straight_link(from, to)
            };
            let points = [
                self.to_screen(viewport, start),
                self.to_screen(viewport, control.0),
                self.to_screen(viewport, control.1),
                self.to_screen(viewport, end),
            ];
            if !bezier_visible(&points, viewport) {
                continue;
            }
            painter.add(CubicBezierShape::from_points_stroke(
                points,
                false,
                Color32::TRANSPARENT,
                Stroke::new(width, color),
            ));
            arrow_head(painter, points[3], points[3] - points[2], width * 3.0, color);
        }
    }

    /// The left and right attachment points of one edge endpoint.
    fn endpoint(&self, endpoint: EndPoint, show_rows: bool) -> Option<Anchor> {
        let rect = self.node_rect(endpoint.file)?;
        let y = match endpoint.symbol {
            Some(index) if show_rows => {
                (rect.top() + arrange::row_offset(index)).min(rect.bottom() - 4.0)
            }
            _ => rect.center().y,
        };
        Some(Anchor {
            left: pos2(rect.left(), y),
            right: pos2(rect.right(), y),
            center: rect.center(),
        })
    }

    // -------------------------------------------------------------- overlay

    fn overlay(&mut self, ui: &mut Ui, viewport: Rect, specbase: &Specbase) {
        let focused = self.focus.is_some();
        let toolbar = Toolbar::new(vec![
            ToolbarGroup::new(vec![
                ToolbarButton::icon("zoom_in", Icon::ZoomIn).tooltip("Zoom in"),
                ToolbarButton::icon("zoom_out", Icon::ZoomOut).tooltip("Zoom out"),
            ]),
            ToolbarGroup::new(vec![
                ToolbarButton::icon("fit", Icon::Fit).tooltip("Fit the whole map in view"),
            ]),
            ToolbarGroup::new(vec![ToolbarButton::icon("focus", Icon::Focus)
                .tooltip(if focused {
                    "Leave focus and show the whole map"
                } else {
                    "Focus on the selected node and its connections"
                })
                .enabled(focused || self.selected.is_some())
                .active(focused)]),
        ])
        .vertical();

        let rect = Rect::from_min_size(viewport.min + vec2(10.0, 10.0), vec2(34.0, 190.0));
        let clicked = ui
            .scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                egui::Frame::new()
                    .fill(theme::PANEL.gamma_multiply(0.92))
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .corner_radius(theme::RADIUS)
                    .inner_margin(3)
                    .show(ui, |ui| toolbar.show(ui))
                    .inner
            })
            .inner;

        match clicked {
            Some("zoom_in") => self.zoom_by(viewport, 1.25, viewport.center()),
            Some("zoom_out") => self.zoom_by(viewport, 0.8, viewport.center()),
            Some("fit") => self.fit(viewport),
            Some("focus") => {
                let target = if focused { None } else { self.selected };
                self.set_focus(specbase, target);
            }
            _ => {}
        }
    }
}

/// The sidebar the canvas opens for the selected node, with the editor in it.
///
/// It is drawn beside the canvas rather than over it, so the map keeps the
/// whole of the room that is left, and its edge can be dragged to say how the
/// two share the room.
pub fn sidebar(ui: &mut Ui, editor: &mut Editor, lsp: Option<&mut Lsp>) -> Option<EditorAction> {
    egui::Panel::right("node_map_sidebar")
        .resizable(true)
        .default_size(theme::SIDEBAR_WIDTH)
        .size_range(theme::SIDEBAR_MIN_WIDTH..=theme::SIDEBAR_MAX_WIDTH)
        .frame(egui::Frame::new().fill(theme::PANEL))
        .show(ui, |ui| {
            ui.painter().line_segment(
                [ui.max_rect().left_top(), ui.max_rect().left_bottom()],
                Stroke::new(1.0, theme::BORDER),
            );
            editor.show(ui, lsp)
        })
        .inner
}

struct Anchor {
    left: Pos2,
    right: Pos2,
    center: Pos2,
}

fn straight_link(from: Anchor, to: Anchor) -> (Pos2, Pos2, (Pos2, Pos2)) {
    let forward = to.center.x >= from.center.x;
    let start = if forward { from.right } else { from.left };
    let end = if forward { to.left } else { to.right };
    let reach = ((end.x - start.x).abs() * 0.5).clamp(36.0, 170.0);
    let direction = if forward { 1.0 } else { -1.0 };
    (
        start,
        end,
        (
            start + vec2(reach * direction, 0.0),
            end - vec2(reach * direction, 0.0),
        ),
    )
}

/// A connection between two symbols in the same file loops out to the side.
fn self_link(from: Anchor, to: Anchor) -> (Pos2, Pos2, (Pos2, Pos2)) {
    let reach = 60.0;
    (
        from.right,
        to.right,
        (
            from.right + vec2(reach, 0.0),
            to.right + vec2(reach, 0.0),
        ),
    )
}

fn bezier_visible(points: &[Pos2; 4], viewport: Rect) -> bool {
    let mut bounds = Rect::from_two_pos(points[0], points[3]);
    bounds = bounds.union(Rect::from_two_pos(points[1], points[2]));
    bounds.intersects(viewport)
}

fn arrow_head(painter: &egui::Painter, tip: Pos2, direction: Vec2, size: f32, color: Color32) {
    if direction.length() < f32::EPSILON || size < 2.0 {
        return;
    }
    let unit = direction.normalized();
    let normal = vec2(-unit.y, unit.x);
    let back = tip - unit * size;
    painter.add(Shape::convex_polygon(
        vec![tip, back + normal * size * 0.4, back - normal * size * 0.4],
        color,
        Stroke::NONE,
    ));
}

fn dashed_rect(painter: &egui::Painter, rect: Rect, stroke: Stroke) {
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
        rect.left_top(),
    ];
    for pair in corners.windows(2) {
        painter.extend(Shape::dashed_line(pair, stroke, 5.0, 4.0));
    }
}

fn badge_text(file: &crate::specbase::SpecFile) -> Option<&'static str> {
    if file.is_config {
        Some("config")
    } else if file.is_entry {
        Some("entry")
    } else {
        None
    }
}
