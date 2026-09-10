//! Loads a directory containing `piton.config.pi` into a [`Specbase`].

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::model::*;
use super::parser::{self, RawStatementKind};

pub const CONFIG_FILE: &str = "piton.config.pi";
const SKIPPED_DIRS: &[&str] = &["target", "node_modules", "dist", "build"];

#[derive(Debug)]
pub enum LoadError {
    NotADirectory(PathBuf),
    MissingConfig(PathBuf),
    Io(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::NotADirectory(p) => write!(f, "{} is not a directory", p.display()),
            LoadError::MissingConfig(p) => {
                write!(f, "No {} found in {}", CONFIG_FILE, p.display())
            }
            LoadError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl Specbase {
    /// Read `dir`, which must contain a `piton.config.pi`.
    pub fn load(dir: &Path) -> Result<Specbase, LoadError> {
        if !dir.is_dir() {
            return Err(LoadError::NotADirectory(dir.to_path_buf()));
        }
        let root = dir
            .canonicalize()
            .map_err(|e| LoadError::Io(e.to_string()))?;
        let config_path = root.join(CONFIG_FILE);
        if !config_path.is_file() {
            return Err(LoadError::MissingConfig(root));
        }

        let mut builder = Builder::new(root.clone());
        builder.walk(&root, None)?;

        let (spec_root, entry_path) = builder.read_config(&config_path);
        builder.spec_root = spec_root;

        builder.resolve_statements();
        builder.build_edges();

        let entry = entry_path.and_then(|p| builder.by_path.get(&p).copied());
        let config_id = builder.by_path.get(&config_path).copied();
        if let Some(id) = config_id {
            builder.files[id].is_config = true;
        }
        if let Some(id) = entry {
            builder.files[id].is_entry = true;
        }

        builder.mark_reachable(config_id.into_iter().chain(entry).collect());

        Ok(Specbase {
            root,
            spec_root: builder.spec_root,
            entry,
            dirs: builder.dirs,
            files: builder.files,
            edges: builder.edges,
            root_dir: 0,
        })
    }
}

struct Builder {
    root: PathBuf,
    spec_root: PathBuf,
    dirs: Vec<SpecDir>,
    files: Vec<SpecFile>,
    edges: Vec<Edge>,
    by_path: HashMap<PathBuf, FileId>,
}

impl Builder {
    fn new(root: PathBuf) -> Self {
        Builder {
            spec_root: root.clone(),
            root,
            dirs: Vec::new(),
            files: Vec::new(),
            edges: Vec::new(),
            by_path: HashMap::new(),
        }
    }

    fn rel(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    /// Recursively record every folder that holds `.pi` files, and the files
    /// themselves. Returns the id of the folder, if it was kept.
    fn walk(&mut self, path: &Path, parent: Option<DirId>) -> Result<Option<DirId>, LoadError> {
        let mut entries: Vec<PathBuf> = fs::read_dir(path)
            .map_err(|e| LoadError::Io(format!("{}: {e}", path.display())))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        entries.sort();

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        let id = self.dirs.len();
        self.dirs.push(SpecDir {
            id,
            name,
            rel: self.rel(path),
            path: path.to_path_buf(),
            parent,
            dirs: Vec::new(),
            files: Vec::new(),
        });

        for entry in &entries {
            let file_name = entry
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if entry.is_dir() {
                if file_name.starts_with('.') || SKIPPED_DIRS.contains(&file_name.as_str()) {
                    continue;
                }
                if let Some(child) = self.walk(entry, Some(id))? {
                    self.dirs[id].dirs.push(child);
                }
            } else if entry.extension().and_then(|e| e.to_str()) == Some("pi") {
                let file_id = self.add_file(entry, id, file_name);
                self.dirs[id].files.push(file_id);
            }
        }

        let empty = self.dirs[id].files.is_empty() && self.dirs[id].dirs.is_empty();
        if empty && parent.is_some() {
            self.dirs.pop();
            return Ok(None);
        }
        Ok(Some(id))
    }

    fn add_file(&mut self, path: &Path, dir: DirId, name: String) -> FileId {
        let source = fs::read_to_string(path).unwrap_or_default();
        let parsed = parser::parse(&source);
        let id = self.files.len();

        let symbols = parsed
            .symbols
            .into_iter()
            .map(|s| Symbol {
                name: s.name,
                kind: s.kind,
                kind_alias: s.kind_alias,
                exported: s.exported,
                is_abstract: s.is_abstract,
                line: s.line,
                fields: s.fields,
                refs: s.refs,
                reached: false,
            })
            .collect();

        let statements = parsed
            .statements
            .into_iter()
            .map(|s| Statement {
                kind: match s.kind {
                    RawStatementKind::Use => StatementKind::Use,
                    RawStatementKind::Import => StatementKind::Import,
                    RawStatementKind::Export => StatementKind::Export,
                },
                external: s.module.starts_with('@'),
                module: s.module,
                items: s
                    .items
                    .into_iter()
                    .map(|i| StatementItem {
                        name: i.name,
                        local: i.local,
                    })
                    .collect(),
                wildcard: s.wildcard,
                line: s.line,
                target: None,
            })
            .collect();

        self.files.push(SpecFile {
            id,
            name,
            rel: self.rel(path),
            path: path.to_path_buf(),
            dir,
            symbols,
            statements,
            reached: false,
            is_config: false,
            is_entry: false,
        });
        self.by_path.insert(path.to_path_buf(), id);
        id
    }

    /// Pull `root` and `entry` out of the `piton-config` declaration.
    fn read_config(&self, config_path: &Path) -> (PathBuf, Option<PathBuf>) {
        let mut spec_root = self.root.clone();
        let mut entry = None;
        if let Some(&id) = self.by_path.get(config_path) {
            for symbol in &self.files[id].symbols {
                if symbol.kind != "piton-config" {
                    continue;
                }
                for (key, value) in &symbol.fields {
                    if value.is_empty() {
                        continue;
                    }
                    match key.as_str() {
                        "root" => spec_root = normalize(&self.root.join(value)),
                        "entry" => entry = Some(normalize(&self.root.join(value))),
                        _ => {}
                    }
                }
            }
        }
        (spec_root, entry)
    }

    /// Point every statement at the file its module specifier names.
    fn resolve_statements(&mut self) {
        for id in 0..self.files.len() {
            let dir = self.files[id]
                .path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| self.root.clone());
            for index in 0..self.files[id].statements.len() {
                if self.files[id].statements[index].external {
                    continue;
                }
                let module = self.files[id].statements[index].module.clone();
                let target = self.resolve_module(&module, &dir);
                self.files[id].statements[index].target = target;
            }
        }
    }

    fn resolve_module(&self, module: &str, from_dir: &Path) -> Option<FileId> {
        let (base, rest) = if let Some(rest) = module.strip_prefix('/') {
            (self.spec_root.clone(), rest.to_string())
        } else {
            (from_dir.to_path_buf(), module.to_string())
        };
        let joined = normalize(&base.join(&rest));
        let candidates = [
            joined.with_extension("pi"),
            joined.join("index.pi"),
            joined.clone(),
        ];
        candidates.iter().find_map(|c| self.by_path.get(c).copied())
    }

    fn build_edges(&mut self) {
        let mut edges: HashSet<Edge> = HashSet::new();

        for file in &self.files {
            // Module level connections.
            for statement in &file.statements {
                let Some(target) = statement.target else { continue };
                if target == file.id {
                    continue;
                }
                let kind = match statement.kind {
                    StatementKind::Use => EdgeKind::Use,
                    _ => EdgeKind::Import,
                };
                if statement.items.is_empty() {
                    edges.insert(Edge {
                        from: EndPoint { file: file.id, symbol: None },
                        to: EndPoint { file: target, symbol: None },
                        kind,
                    });
                }
                for item in &statement.items {
                    edges.insert(Edge {
                        from: EndPoint { file: file.id, symbol: None },
                        to: EndPoint {
                            file: target,
                            symbol: self.files[target].symbol_index(&item.name),
                        },
                        kind,
                    });
                }
            }

            // Connections written inside declaration bodies.
            let bindings = self.bindings(file);
            for (index, symbol) in file.symbols.iter().enumerate() {
                for name in &symbol.refs {
                    if let Some(to) = self.lookup(file, &bindings, name) {
                        if to.file == file.id && to.symbol == Some(index) {
                            continue;
                        }
                        edges.insert(Edge {
                            from: EndPoint { file: file.id, symbol: Some(index) },
                            to,
                            kind: EdgeKind::Reference,
                        });
                    }
                }
                // A declaration written with a keyword brought in by `use`
                // depends on the declaration that defines that keyword.
                if let Some(to) = self.lookup_keyword(file, &symbol.kind) {
                    edges.insert(Edge {
                        from: EndPoint { file: file.id, symbol: Some(index) },
                        to,
                        kind: EdgeKind::Use,
                    });
                }
            }
        }

        let mut edges: Vec<Edge> = edges.into_iter().collect();
        edges.sort_by_key(|e| (e.from.file, e.from.symbol, e.to.file, e.to.symbol));
        self.edges = edges;
    }

    /// Local name -> (module target, exported name) for one file.
    fn bindings(&self, file: &SpecFile) -> HashMap<String, (FileId, String)> {
        let mut map = HashMap::new();
        for statement in &file.statements {
            let Some(target) = statement.target else { continue };
            for item in &statement.items {
                map.insert(item.local.clone(), (target, item.name.clone()));
            }
        }
        map
    }

    fn lookup(
        &self,
        file: &SpecFile,
        bindings: &HashMap<String, (FileId, String)>,
        name: &str,
    ) -> Option<EndPoint> {
        if let Some(index) = file.symbol_index(name) {
            return Some(EndPoint { file: file.id, symbol: Some(index) });
        }
        let (target, exported) = bindings.get(name)?;
        let (file_id, symbol) = self
            .resolve_export(*target, exported, &mut HashSet::new())
            .unwrap_or((*target, usize::MAX));
        Some(EndPoint {
            file: file_id,
            symbol: (symbol != usize::MAX).then_some(symbol),
        })
    }

    /// Find the declaration a `use`d keyword such as `build-skill` comes from.
    fn lookup_keyword(&self, file: &SpecFile, keyword: &str) -> Option<EndPoint> {
        for statement in &file.statements {
            if statement.kind != StatementKind::Use {
                continue;
            }
            let target = statement.target?;
            let index = self.files[target].symbols.iter().position(|s| {
                s.kind_alias.as_deref() == Some(keyword)
            })?;
            return Some(EndPoint { file: target, symbol: Some(index) });
        }
        None
    }

    /// Follow re-exports until the file that actually declares `name`.
    fn resolve_export(
        &self,
        file: FileId,
        name: &str,
        seen: &mut HashSet<FileId>,
    ) -> Option<(FileId, usize)> {
        if !seen.insert(file) {
            return None;
        }
        if let Some(index) = self.files[file].symbol_index(name) {
            return Some((file, index));
        }
        for statement in &self.files[file].statements {
            if statement.kind != StatementKind::Export {
                continue;
            }
            let Some(target) = statement.target else { continue };
            if statement.wildcard {
                if let Some(found) = self.resolve_export(target, name, seen) {
                    return Some(found);
                }
            }
            for item in &statement.items {
                if item.local == name {
                    if let Some(found) = self.resolve_export(target, &item.name, seen) {
                        return Some(found);
                    }
                }
            }
        }
        None
    }

    /// Walk out from the project roots so unreached files and symbols can be
    /// told apart from the rest.
    fn mark_reachable(&mut self, roots: Vec<FileId>) {
        let mut files: HashSet<FileId> = HashSet::new();
        let mut symbols: HashSet<(FileId, usize)> = HashSet::new();
        for root in roots {
            self.demand_module(root, &mut files, &mut symbols);
        }
        for file in &mut self.files {
            file.reached = files.contains(&file.id);
            for (index, symbol) in file.symbols.iter_mut().enumerate() {
                symbol.reached = symbols.contains(&(file.id, index));
            }
        }
    }

    /// Everything this module exports is needed.
    fn demand_module(
        &self,
        file: FileId,
        files: &mut HashSet<FileId>,
        symbols: &mut HashSet<(FileId, usize)>,
    ) {
        let first_visit = files.insert(file);
        if first_visit {
            self.reach_file(file, files, symbols);
        }
        for index in 0..self.files[file].symbols.len() {
            if self.files[file].symbols[index].exported {
                self.demand_symbol(file, index, files, symbols);
            }
        }
        for statement in &self.files[file].statements {
            if statement.kind != StatementKind::Export {
                continue;
            }
            let Some(target) = statement.target else { continue };
            if statement.wildcard {
                self.demand_module(target, files, symbols);
            }
        }
    }

    /// Mark a file as part of the graph and follow its statements.
    fn reach_file(
        &self,
        file: FileId,
        files: &mut HashSet<FileId>,
        symbols: &mut HashSet<(FileId, usize)>,
    ) {
        files.insert(file);
        for statement in &self.files[file].statements {
            let Some(target) = statement.target else { continue };
            if files.insert(target) {
                self.reach_file(target, files, symbols);
            }
            if statement.kind == StatementKind::Use {
                continue;
            }
            if statement.wildcard {
                self.demand_module(target, files, symbols);
            }
            for item in &statement.items {
                if let Some((f, i)) = self.resolve_export(target, &item.name, &mut HashSet::new()) {
                    self.demand_symbol(f, i, files, symbols);
                }
            }
        }
    }

    fn demand_symbol(
        &self,
        file: FileId,
        index: usize,
        files: &mut HashSet<FileId>,
        symbols: &mut HashSet<(FileId, usize)>,
    ) {
        if files.insert(file) {
            self.reach_file(file, files, symbols);
        }
        if !symbols.insert((file, index)) {
            return;
        }
        for edge in self.symbol_targets(file, index) {
            match edge.symbol {
                Some(target_index) => self.demand_symbol(edge.file, target_index, files, symbols),
                None => {
                    if files.insert(edge.file) {
                        self.reach_file(edge.file, files, symbols);
                    }
                }
            }
        }
    }

    /// The endpoints one declaration reaches through its body references and
    /// its declaration keyword.
    fn symbol_targets(&self, file: FileId, index: usize) -> Vec<EndPoint> {
        let source = &self.files[file];
        let bindings = self.bindings(source);
        let mut out = Vec::new();
        for name in &source.symbols[index].refs {
            if let Some(point) = self.lookup(source, &bindings, name) {
                out.push(point);
            }
        }
        if let Some(point) = self.lookup_keyword(source, &source.symbols[index].kind) {
            out.push(point);
        }
        out
    }
}

/// Resolve `.` and `..` without touching the filesystem.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Specbase {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        Specbase::load(&root).expect("this project is a Piton project")
    }

    #[test]
    fn reads_the_config() {
        let specbase = fixture();
        assert_eq!(specbase.spec_root, specbase.root.join("spec"));
        let entry = specbase.entry.expect("entry is declared in the config");
        assert_eq!(specbase.file(entry).rel, "spec/index.pi");
    }

    #[test]
    fn finds_every_pi_file() {
        let specbase = fixture();
        let names: Vec<&str> = specbase.files.iter().map(|f| f.rel.as_str()).collect();
        assert!(names.contains(&"piton.config.pi"));
        assert!(names.contains(&"spec/shape/ui/Layout.pi"));
        assert!(names.contains(&"spec/lib/build-skill.pi"));
    }

    #[test]
    fn reads_declarations() {
        let specbase = fixture();
        let file = specbase
            .files
            .iter()
            .find(|f| f.rel == "spec/lib/build-skill.pi")
            .unwrap();
        let symbol = &file.symbols[0];
        assert_eq!(symbol.name, "BuildSkill");
        assert_eq!(symbol.kind, "skill");
        assert_eq!(symbol.kind_alias.as_deref(), Some("build-skill"));
        assert!(symbol.is_abstract);
        assert!(symbol.exported);
    }

    #[test]
    fn resolves_relative_and_rooted_modules() {
        let specbase = fixture();
        let layout = specbase
            .files
            .iter()
            .find(|f| f.rel == "spec/shape/ui/Layout.pi")
            .unwrap();
        for statement in &layout.statements {
            let target = statement.target.expect("resolves inside the specbase");
            assert!(specbase.file(target).rel.starts_with("spec/shape/ui/"));
        }

        let skill = specbase
            .files
            .iter()
            .find(|f| f.rel == "spec/agent/skills/BuildApplication.pi")
            .unwrap();
        let rooted = skill
            .statements
            .iter()
            .find(|s| s.module == "/shape/Application")
            .unwrap();
        let target = rooted.target.unwrap();
        assert_eq!(specbase.file(target).rel, "spec/shape/Application.pi");
    }

    #[test]
    fn connects_anchors_through_body_references() {
        let specbase = fixture();
        let layout = specbase
            .files
            .iter()
            .find(|f| f.rel == "spec/shape/ui/Layout.pi")
            .unwrap();
        let toolbar = specbase
            .files
            .iter()
            .find(|f| f.rel == "spec/shape/ui/Toolbar.pi")
            .unwrap();

        let connected = specbase.edges.iter().any(|e| {
            e.kind == EdgeKind::Reference
                && e.from == EndPoint { file: layout.id, symbol: Some(0) }
                && e.to == EndPoint { file: toolbar.id, symbol: Some(0) }
        });
        assert!(connected, "Layout references the Toolbar anchor");
    }

    #[test]
    fn a_used_keyword_connects_to_its_declaration() {
        let specbase = fixture();
        let skill = specbase
            .files
            .iter()
            .find(|f| f.rel == "spec/agent/skills/BuildApplication.pi")
            .unwrap();
        let library = specbase
            .files
            .iter()
            .find(|f| f.rel == "spec/lib/build-skill.pi")
            .unwrap();
        let connected = specbase.edges.iter().any(|e| {
            e.kind == EdgeKind::Use
                && e.from.file == skill.id
                && e.to == EndPoint { file: library.id, symbol: Some(0) }
        });
        assert!(connected, "`build-skill` comes from the BuildSkill declaration");
    }

    #[test]
    fn follows_re_exports_when_reaching_symbols() {
        let specbase = fixture();
        for file in &specbase.files {
            assert!(file.reached, "{} should be reachable", file.rel);
        }
        let concept = specbase
            .files
            .iter()
            .find(|f| f.rel == "spec/concept/ui/NodeMap.pi")
            .unwrap();
        assert!(
            concept.symbols[0].reached,
            "NodeMap is reached through the concept index"
        );
    }

    #[test]
    fn a_directory_without_a_config_is_rejected() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        assert!(matches!(
            Specbase::load(&root),
            Err(LoadError::MissingConfig(_))
        ));
    }
}
