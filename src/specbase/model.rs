//! The in-memory model of a specbase: the folders, files, symbols and the
//! connections between them that the node map draws.
//!
//! The model keeps everything the parser recovers, including source positions
//! and paths the current views do not put on screen.
#![allow(dead_code)]

use std::path::PathBuf;

pub type DirId = usize;
pub type FileId = usize;

/// A folder inside the opened specbase.
#[derive(Debug, Clone)]
pub struct SpecDir {
    pub id: DirId,
    pub name: String,
    pub rel: String,
    pub path: PathBuf,
    pub parent: Option<DirId>,
    pub dirs: Vec<DirId>,
    pub files: Vec<FileId>,
}

/// A single `.pi` file.
#[derive(Debug, Clone)]
pub struct SpecFile {
    pub id: FileId,
    pub name: String,
    pub rel: String,
    pub path: PathBuf,
    pub dir: DirId,
    pub symbols: Vec<Symbol>,
    pub statements: Vec<Statement>,
    /// True when the file is reachable from the project entry point.
    pub reached: bool,
    pub is_config: bool,
    pub is_entry: bool,
}

impl SpecFile {
    pub fn symbol_index(&self, name: &str) -> Option<usize> {
        self.symbols.iter().position(|s| s.name == name)
    }
}

/// A top level declaration inside a `.pi` file.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    /// The declaration keyword, e.g. `anchor`, `skill`, `piton-config`.
    pub kind: String,
    /// The keyword this declaration can be used as, from `... as <alias>`.
    pub kind_alias: Option<String>,
    pub exported: bool,
    pub is_abstract: bool,
    pub line: usize,
    /// Single line `key: value` pairs directly inside the declaration.
    pub fields: Vec<(String, String)>,
    /// Names referenced from the body via `@{...}`, `{...}` or `${...}`.
    pub refs: Vec<String>,
    pub reached: bool,
}

/// How a module level statement links one file to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementKind {
    Use,
    Import,
    Export,
}

/// A `use` / `from ... import ...` / `from ... export ...` statement.
#[derive(Debug, Clone)]
pub struct Statement {
    pub kind: StatementKind,
    /// The module specifier exactly as written, e.g. `./ui/Layout`.
    pub module: String,
    pub items: Vec<StatementItem>,
    pub wildcard: bool,
    pub line: usize,
    /// The file the module specifier resolves to, when it is in this specbase.
    pub target: Option<FileId>,
    /// True when the specifier points at an external package (`@scope/name`).
    pub external: bool,
}

#[derive(Debug, Clone)]
pub struct StatementItem {
    /// The exported name in the target module.
    pub name: String,
    /// The name it is bound to locally.
    pub local: String,
}

/// One end of a connection: a file, and optionally a symbol inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EndPoint {
    pub file: FileId,
    pub symbol: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    /// A module level dependency created by an import/export statement.
    Import,
    /// A `use` statement, or a declaration written with a used keyword.
    Use,
    /// A `@{...}` reference from inside a declaration body.
    Reference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Edge {
    pub from: EndPoint,
    pub to: EndPoint,
    pub kind: EdgeKind,
}

/// Everything loaded from one opened directory.
#[derive(Debug, Clone)]
pub struct Specbase {
    /// The directory the user opened; contains `piton.config.pi`.
    pub root: PathBuf,
    /// The spec root that `/`-prefixed module specifiers resolve against.
    pub spec_root: PathBuf,
    pub entry: Option<FileId>,
    pub dirs: Vec<SpecDir>,
    pub files: Vec<SpecFile>,
    pub edges: Vec<Edge>,
    pub root_dir: DirId,
}

impl Specbase {
    pub fn file(&self, id: FileId) -> &SpecFile {
        &self.files[id]
    }

    pub fn dir(&self, id: DirId) -> &SpecDir {
        &self.dirs[id]
    }

    pub fn symbol_count(&self) -> usize {
        self.files.iter().map(|f| f.symbols.len()).sum()
    }

    pub fn unreached_files(&self) -> usize {
        self.files.iter().filter(|f| !f.reached).count()
    }

    pub fn unreached_symbols(&self) -> usize {
        self.files
            .iter()
            .flat_map(|f| f.symbols.iter())
            .filter(|s| !s.reached)
            .count()
    }
}
