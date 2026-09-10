//! Turns the folder/file tree of a specbase into boxes on the canvas.
//!
//! Folders become containers, files become nodes with one row per symbol.
//!
//! Focusing on a node throws that layout away and builds a much smaller one:
//! the focused node and the nodes it connects to, and nothing else.

use eframe::egui::{pos2, vec2, Rect, Vec2};

use crate::specbase::{DirId, FileId, Specbase};

pub const FILE_WIDTH: f32 = 252.0;
pub const FILE_HEADER: f32 = 30.0;
pub const ROW_HEIGHT: f32 = 20.0;
pub const FILE_FOOTER: f32 = 8.0;
pub const GAP: f32 = 22.0;
pub const DIR_PADDING: f32 = 16.0;
pub const DIR_HEADER: f32 = 24.0;
/// The space between the columns of a focused layout.
pub const FOCUS_GAP: f32 = 140.0;
const MAX_COLUMN: f32 = 1000.0;

#[derive(Debug, Clone, Copy)]
pub struct NodeBox {
    pub file: FileId,
    pub rect: Rect,
}

#[derive(Debug, Clone, Copy)]
pub struct GroupBox {
    pub dir: DirId,
    pub rect: Rect,
    pub depth: usize,
}

#[derive(Debug, Clone)]
pub struct Arrangement {
    pub nodes: Vec<NodeBox>,
    pub groups: Vec<GroupBox>,
    /// Index into `nodes`, by file id.
    pub node_of_file: Vec<usize>,
    pub bounds: Rect,
}

impl Default for Arrangement {
    fn default() -> Self {
        Arrangement {
            nodes: Vec::new(),
            groups: Vec::new(),
            node_of_file: Vec::new(),
            bounds: Rect::from_min_size(pos2(0.0, 0.0), vec2(1.0, 1.0)),
        }
    }
}

impl Arrangement {
    /// The box laid out for a file, when this arrangement shows that file.
    pub fn node(&self, file: FileId) -> Option<&NodeBox> {
        match self.node_of_file.get(file) {
            Some(&index) if index != MISSING => Some(&self.nodes[index]),
            _ => None,
        }
    }

    #[allow(dead_code)] // Read by the layout tests.
    pub fn shows(&self, file: FileId) -> bool {
        self.node(file).is_some()
    }

    /// The files this arrangement lays out, in drawing order.
    pub fn files(&self) -> impl Iterator<Item = FileId> + '_ {
        self.nodes.iter().map(|node| node.file)
    }
}

/// The slot of a file that an arrangement does not lay out.
const MISSING: usize = usize::MAX;

enum Kind {
    Dir(DirId),
    File(FileId),
}

struct Tree {
    kind: Kind,
    size: Vec2,
    children: Vec<(Vec2, Tree)>,
}

pub fn arrange(specbase: &Specbase) -> Arrangement {
    let tree = build_dir(specbase, specbase.root_dir);
    let mut arrangement = Arrangement {
        node_of_file: vec![MISSING; specbase.files.len()],
        ..Default::default()
    };
    flatten(&tree, Vec2::ZERO, 0, &mut arrangement);

    arrangement.bounds = arrangement
        .groups
        .first()
        .map(|g| g.rect)
        .or_else(|| arrangement.nodes.first().map(|n| n.rect))
        .unwrap_or(Rect::from_min_size(pos2(0.0, 0.0), vec2(1.0, 1.0)));
    for node in &arrangement.nodes {
        arrangement.bounds = arrangement.bounds.union(node.rect);
    }
    for group in &arrangement.groups {
        arrangement.bounds = arrangement.bounds.union(group.rect);
    }
    arrangement
}

pub fn file_size(specbase: &Specbase, file: FileId) -> Vec2 {
    let rows = specbase.files[file].symbols.len().max(1) as f32;
    vec2(FILE_WIDTH, FILE_HEADER + rows * ROW_HEIGHT + FILE_FOOTER)
}

/// The centre line of a symbol row, relative to the top of a file node.
pub fn row_offset(index: usize) -> f32 {
    FILE_HEADER + index as f32 * ROW_HEIGHT + ROW_HEIGHT / 2.0
}

/// Lay out one file and the files it connects to, and nothing else.
///
/// The focused file sits in the middle, the files that point at it stand in a
/// column on the left, and the files it points at stand in a column on the
/// right, so the connections read left to right.
pub fn focus(specbase: &Specbase, file: FileId) -> Arrangement {
    let (incoming, outgoing) = neighbours(specbase, file);
    let mut arrangement = Arrangement {
        node_of_file: vec![MISSING; specbase.files.len()],
        ..Default::default()
    };

    let column = |index: usize| index as f32 * (FILE_WIDTH + FOCUS_GAP);
    place_column(specbase, &incoming, column(0), &mut arrangement);
    place_column(specbase, &[file], column(1), &mut arrangement);
    place_column(specbase, &outgoing, column(2), &mut arrangement);

    arrangement.bounds = arrangement
        .nodes
        .first()
        .map(|node| node.rect)
        .unwrap_or(Rect::from_min_size(pos2(0.0, 0.0), vec2(1.0, 1.0)));
    for node in &arrangement.nodes {
        arrangement.bounds = arrangement.bounds.union(node.rect);
    }
    arrangement
}

/// The files that point at `file`, and the files `file` points at.
///
/// A file on both sides is only reported as outgoing, so it is laid out once.
pub fn neighbours(specbase: &Specbase, file: FileId) -> (Vec<FileId>, Vec<FileId>) {
    let mut incoming = Vec::new();
    let mut outgoing = Vec::new();
    for edge in &specbase.edges {
        if edge.from.file == file && edge.to.file != file {
            outgoing.push(edge.to.file);
        } else if edge.to.file == file && edge.from.file != file {
            incoming.push(edge.from.file);
        }
    }
    outgoing.sort_unstable();
    outgoing.dedup();
    incoming.sort_unstable();
    incoming.dedup();
    incoming.retain(|id| !outgoing.contains(id));
    (incoming, outgoing)
}

/// Stack files in a column, centred on the horizontal axis of the layout.
fn place_column(specbase: &Specbase, files: &[FileId], x: f32, out: &mut Arrangement) {
    if files.is_empty() {
        return;
    }
    let sizes: Vec<Vec2> = files.iter().map(|&f| file_size(specbase, f)).collect();
    let height: f32 =
        sizes.iter().map(|s| s.y).sum::<f32>() + GAP * (files.len() - 1) as f32;

    let mut y = -height / 2.0;
    for (&file, size) in files.iter().zip(&sizes) {
        out.node_of_file[file] = out.nodes.len();
        out.nodes.push(NodeBox {
            file,
            rect: Rect::from_min_size(pos2(x, y), *size),
        });
        y += size.y + GAP;
    }
}

fn build_dir(specbase: &Specbase, dir: DirId) -> Tree {
    let mut children: Vec<Tree> = Vec::new();
    for &file in &specbase.dirs[dir].files {
        children.push(Tree {
            kind: Kind::File(file),
            size: file_size(specbase, file),
            children: Vec::new(),
        });
    }
    for &child in &specbase.dirs[dir].dirs {
        children.push(build_dir(specbase, child));
    }

    let (placed, content) = pack(children);
    let origin = vec2(DIR_PADDING, DIR_HEADER + DIR_PADDING * 0.6);
    let size = vec2(
        content.x + DIR_PADDING * 2.0,
        content.y + origin.y + DIR_PADDING,
    );
    Tree {
        kind: Kind::Dir(dir),
        size: size.max(vec2(FILE_WIDTH + DIR_PADDING * 2.0, DIR_HEADER + DIR_PADDING * 2.0)),
        children: placed
            .into_iter()
            .map(|(offset, tree)| (offset + origin, tree))
            .collect(),
    }
}

/// Stack children into a column, starting a new column when one grows too tall.
fn pack(children: Vec<Tree>) -> (Vec<(Vec2, Tree)>, Vec2) {
    let mut placed = Vec::new();
    let mut cursor = Vec2::ZERO;
    let mut column_width: f32 = 0.0;
    let mut extent = Vec2::ZERO;

    for child in children {
        if cursor.y > 0.0 && cursor.y + child.size.y > MAX_COLUMN {
            cursor = vec2(cursor.x + column_width + GAP, 0.0);
            column_width = 0.0;
        }
        let offset = cursor;
        cursor.y += child.size.y + GAP;
        column_width = column_width.max(child.size.x);
        extent = extent.max(offset + child.size);
        placed.push((offset, child));
    }
    (placed, extent)
}

fn flatten(tree: &Tree, origin: Vec2, depth: usize, out: &mut Arrangement) {
    let rect = Rect::from_min_size(origin.to_pos2(), tree.size);
    match tree.kind {
        Kind::Dir(dir) => out.groups.push(GroupBox { dir, rect, depth }),
        Kind::File(file) => {
            out.node_of_file[file] = out.nodes.len();
            out.nodes.push(NodeBox { file, rect });
        }
    }
    for (offset, child) in &tree.children {
        flatten(child, origin + *offset, depth + 1, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture() -> Specbase {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        Specbase::load(&root).expect("this project is a Piton project")
    }

    fn file_id(specbase: &Specbase, rel: &str) -> FileId {
        specbase
            .files
            .iter()
            .find(|file| file.rel == rel)
            .unwrap_or_else(|| panic!("no file called {rel}"))
            .id
    }

    #[test]
    fn a_node_is_tall_enough_for_its_declarations() {
        let specbase = fixture();
        let empty = file_size(&specbase, file_id(&specbase, "spec/agent/commands/index.pi"));
        let full = file_size(&specbase, file_id(&specbase, "piton.config.pi"));
        assert_eq!(empty.x, full.x, "every node is the same width");
        assert!(full.y > empty.y, "more declarations, taller node");
        assert!(
            empty.y >= FILE_HEADER + ROW_HEIGHT,
            "a node with no declarations still has room for a row"
        );
    }

    #[test]
    fn rows_are_evenly_spaced_below_the_header() {
        assert_eq!(row_offset(1) - row_offset(0), ROW_HEIGHT);
        assert!(row_offset(0) > FILE_HEADER);
    }

    #[test]
    fn every_file_gets_a_node_of_its_own() {
        let specbase = fixture();
        let arrangement = arrange(&specbase);
        assert_eq!(arrangement.nodes.len(), specbase.files.len());
        for file in 0..specbase.files.len() {
            assert!(arrangement.shows(file), "file {file} is laid out");
        }
    }

    #[test]
    fn nodes_never_overlap_each_other() {
        let specbase = fixture();
        let arrangement = arrange(&specbase);
        for (index, node) in arrangement.nodes.iter().enumerate() {
            for other in &arrangement.nodes[index + 1..] {
                let overlap = node.rect.intersect(other.rect);
                assert!(
                    overlap.width() <= 0.0 || overlap.height() <= 0.0,
                    "{:?} and {:?} overlap",
                    node.rect,
                    other.rect
                );
            }
        }
    }

    #[test]
    fn a_folder_box_contains_the_files_it_holds() {
        let specbase = fixture();
        let arrangement = arrange(&specbase);
        for group in &arrangement.groups {
            for &file in &specbase.dir(group.dir).files {
                let node = arrangement.node(file).unwrap();
                assert!(
                    group.rect.contains_rect(node.rect),
                    "{} sits inside {}",
                    specbase.file(file).rel,
                    specbase.dir(group.dir).rel
                );
            }
        }
    }

    #[test]
    fn the_bounds_hold_everything_that_was_laid_out() {
        let specbase = fixture();
        let arrangement = arrange(&specbase);
        for node in &arrangement.nodes {
            assert!(arrangement.bounds.contains_rect(node.rect));
        }
        for group in &arrangement.groups {
            assert!(arrangement.bounds.contains_rect(group.rect));
        }
    }

    #[test]
    fn focusing_lays_out_the_node_and_its_connections_and_nothing_else() {
        let specbase = fixture();
        let layout = file_id(&specbase, "spec/shape/ui/Layout.pi");
        let arrangement = focus(&specbase, layout);

        let (incoming, outgoing) = neighbours(&specbase, layout);
        assert!(arrangement.shows(layout));
        assert_eq!(arrangement.nodes.len(), incoming.len() + outgoing.len() + 1);
        for file in incoming.iter().chain(&outgoing) {
            assert!(arrangement.shows(*file));
        }

        let unrelated = file_id(&specbase, "spec/agent/skills/Testing.pi");
        assert!(!arrangement.shows(unrelated), "the rest disappears");
        assert!(arrangement.groups.is_empty(), "folders disappear too");
    }

    #[test]
    fn a_focused_layout_reads_left_to_right() {
        let specbase = fixture();
        let layout = file_id(&specbase, "spec/shape/ui/Layout.pi");
        let arrangement = focus(&specbase, layout);
        let (incoming, outgoing) = neighbours(&specbase, layout);
        let middle = arrangement.node(layout).unwrap().rect;

        for file in incoming {
            let rect = arrangement.node(file).unwrap().rect;
            assert!(rect.right() < middle.left(), "what points at it is left");
        }
        for file in outgoing {
            let rect = arrangement.node(file).unwrap().rect;
            assert!(rect.left() > middle.right(), "what it points at is right");
        }
    }

    #[test]
    fn a_focused_layout_stacks_a_column_without_overlaps() {
        let specbase = fixture();
        let index = file_id(&specbase, "spec/shape/index.pi");
        let arrangement = focus(&specbase, index);

        for (position, node) in arrangement.nodes.iter().enumerate() {
            for other in &arrangement.nodes[position + 1..] {
                let overlap = node.rect.intersect(other.rect);
                assert!(overlap.width() <= 0.0 || overlap.height() <= 0.0);
            }
        }
    }

    #[test]
    fn a_file_is_never_laid_out_twice_when_it_points_both_ways() {
        let specbase = fixture();
        for file in 0..specbase.files.len() {
            let (incoming, outgoing) = neighbours(&specbase, file);
            for id in &incoming {
                assert!(
                    !outgoing.contains(id),
                    "a neighbour belongs to one column only"
                );
            }
            assert!(!incoming.contains(&file) && !outgoing.contains(&file));
        }
    }

    #[test]
    fn an_arrangement_reports_nothing_for_a_file_it_does_not_show() {
        let specbase = fixture();
        let layout = file_id(&specbase, "spec/shape/ui/Layout.pi");
        let unrelated = file_id(&specbase, "spec/agent/skills/Testing.pi");
        let arrangement = focus(&specbase, layout);
        assert!(arrangement.node(unrelated).is_none());
        assert!(arrangement.node(specbase.files.len() + 10).is_none());
    }
}
