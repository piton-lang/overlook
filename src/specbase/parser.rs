//! A lightweight structural parser for `.pi` files.
//!
//! It reads only what the node map needs: the module level statements, the top
//! level declarations, their first level fields, and the symbol references
//! written inside their bodies.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawStatementKind {
    Use,
    Import,
    Export,
}

#[derive(Debug, Clone)]
pub struct RawItem {
    pub name: String,
    pub local: String,
}

#[derive(Debug, Clone)]
pub struct RawStatement {
    pub kind: RawStatementKind,
    pub module: String,
    pub items: Vec<RawItem>,
    pub wildcard: bool,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct RawSymbol {
    pub name: String,
    pub kind: String,
    pub kind_alias: Option<String>,
    pub exported: bool,
    pub is_abstract: bool,
    pub line: usize,
    pub fields: Vec<(String, String)>,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ParsedFile {
    pub statements: Vec<RawStatement>,
    pub symbols: Vec<RawSymbol>,
}

pub fn parse(source: &str) -> ParsedFile {
    let mut out = ParsedFile::default();
    let mut current: Option<usize> = None;

    for (idx, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim_end();
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - trimmed.len();
        let line_no = idx + 1;

        if indent == 0 {
            if let Some(stmt) = parse_statement(trimmed, line_no) {
                current = None;
                out.statements.push(stmt);
                continue;
            }
            if let Some(sym) = parse_declaration(trimmed, line_no) {
                out.symbols.push(sym);
                current = Some(out.symbols.len() - 1);
                continue;
            }
            current = None;
            continue;
        }

        let Some(sym_idx) = current else { continue };
        let sym = &mut out.symbols[sym_idx];
        extract_refs(trimmed, &mut sym.refs);
        if let Some((key, value)) = parse_field(trimmed) {
            // Only record fields written directly inside the declaration.
            if indent <= 4 {
                sym.fields.push((key, value));
            }
        }
    }

    for sym in &mut out.symbols {
        sym.refs.sort();
        sym.refs.dedup();
    }
    out
}

/// `use <module>` / `from <module> import ...` / `from <module> export ...`
fn parse_statement(line: &str, line_no: usize) -> Option<RawStatement> {
    let mut words = line.split_whitespace();
    match words.next()? {
        "use" => {
            let module = words.next()?.to_string();
            Some(RawStatement {
                kind: RawStatementKind::Use,
                module,
                items: Vec::new(),
                wildcard: false,
                line: line_no,
            })
        }
        "from" => {
            let module = words.next()?.to_string();
            let kind = match words.next()? {
                "import" => RawStatementKind::Import,
                "export" => RawStatementKind::Export,
                _ => return None,
            };
            let rest: String = words.collect::<Vec<_>>().join(" ");
            let (items, wildcard) = parse_items(&rest);
            Some(RawStatement {
                kind,
                module,
                items,
                wildcard,
                line: line_no,
            })
        }
        _ => None,
    }
}

/// `A`, `A B` (import `A` bound locally as `B`), `A, B`, or `*`.
fn parse_items(rest: &str) -> (Vec<RawItem>, bool) {
    let mut items = Vec::new();
    let mut wildcard = false;
    for part in rest.split(',') {
        let words: Vec<&str> = part.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }
        if words[0] == "*" {
            wildcard = true;
            continue;
        }
        let name = words[0].to_string();
        let local = words.get(1).map(|s| s.to_string()).unwrap_or_else(|| name.clone());
        items.push(RawItem { name, local });
    }
    (items, wildcard)
}

/// `[export] [abstract] <kind> <Name> [as <alias>]:`
fn parse_declaration(line: &str, line_no: usize) -> Option<RawSymbol> {
    let head = line.strip_suffix(':')?;
    if head.ends_with(':') {
        // `field:: type` declarations are not top level symbols.
        return None;
    }
    let mut words: Vec<&str> = head.split_whitespace().collect();

    let mut exported = false;
    let mut is_abstract = false;
    if words.first() == Some(&"export") {
        exported = true;
        words.remove(0);
    }
    if words.first() == Some(&"abstract") {
        is_abstract = true;
        words.remove(0);
    }

    let mut kind_alias = None;
    if words.len() >= 2 && words[words.len() - 2] == "as" {
        kind_alias = Some(words[words.len() - 1].to_string());
        words.truncate(words.len() - 2);
    }

    if words.len() != 2 {
        return None;
    }
    let kind = words[0];
    let name = words[1];
    if !is_keyword(kind) || !is_name(name) {
        return None;
    }

    Some(RawSymbol {
        name: name.to_string(),
        kind: kind.to_string(),
        kind_alias,
        exported,
        is_abstract,
        line: line_no,
        fields: Vec::new(),
        refs: Vec::new(),
    })
}

/// Declaration keywords are lower case and may be hyphenated: `build-skill`.
fn is_keyword(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    word.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

fn is_name(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    word.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

fn parse_field(line: &str) -> Option<(String, String)> {
    let colon = line.find(':')?;
    let key = line[..colon].trim();
    if key.is_empty() || !is_name(key) && !is_keyword(key) {
        return None;
    }
    let mut value = line[colon + 1..].trim_start();
    // `name:: type` declares a field type rather than a value.
    if let Some(stripped) = value.strip_prefix(':') {
        value = stripped.trim_start();
    }
    Some((key.to_string(), value.trim_end().to_string()))
}

/// Collect `@{Name}`, `{Name}` and `${Name}` references from a line.
pub fn extract_refs(line: &str, out: &mut Vec<String>) {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'{' {
            i += 1;
            continue;
        }
        let Some(offset) = line[i..].find('}') else { break };
        let inner = line[i + 1..i + offset].trim();
        let templated = i > 0 && bytes[i - 1] == b'$';
        if is_reference_body(inner) && !(templated && inner.contains('.')) {
            out.push(inner.to_string());
        }
        i += offset + 1;
    }
}

fn is_reference_body(inner: &str) -> bool {
    if inner.is_empty() {
        return false;
    }
    inner
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_statements() {
        let parsed = parse(
            "use @piton/belay\n\
             from ./Stack import Stack\n\
             from /concept import Application Concept\n\
             from ./skills export *\n",
        );
        assert_eq!(parsed.statements.len(), 4);
        assert_eq!(parsed.statements[0].kind, RawStatementKind::Use);
        assert_eq!(parsed.statements[0].module, "@piton/belay");
        assert_eq!(parsed.statements[2].items[0].name, "Application");
        assert_eq!(parsed.statements[2].items[0].local, "Concept");
        assert!(parsed.statements[3].wildcard);
        assert_eq!(parsed.statements[3].kind, RawStatementKind::Export);
    }

    #[test]
    fn parses_declarations_and_fields() {
        let parsed = parse(
            "export anchor Stack:\n    language: Rust\n    platforms:\n        - Windows\n",
        );
        assert_eq!(parsed.symbols.len(), 1);
        let symbol = &parsed.symbols[0];
        assert_eq!(symbol.kind, "anchor");
        assert_eq!(symbol.name, "Stack");
        assert!(symbol.exported);
        assert_eq!(symbol.fields[0], ("language".into(), "Rust".into()));
        assert_eq!(symbol.fields[1], ("platforms".into(), String::new()));
    }

    #[test]
    fn a_field_type_is_not_a_declaration() {
        let parsed = parse("export skill S:\n    scope:: string\n");
        assert_eq!(parsed.symbols.len(), 1);
        assert_eq!(parsed.symbols[0].fields[0], ("scope".into(), "string".into()));
    }

    #[test]
    fn collects_references() {
        let mut refs = Vec::new();
        extract_refs("        A @{NodeMap} and {Stack} and ${self.scope}", &mut refs);
        assert_eq!(refs, vec!["NodeMap", "Stack"]);

        let mut refs = Vec::new();
        extract_refs("Use ${__BELAY_SHAPE__} to guide", &mut refs);
        assert_eq!(refs, vec!["__BELAY_SHAPE__"]);
    }

    #[test]
    fn keeps_the_declaration_keyword_alias() {
        let parsed = parse("export abstract skill BuildSkill as build-skill:\n");
        let symbol = &parsed.symbols[0];
        assert!(symbol.is_abstract);
        assert_eq!(symbol.kind_alias.as_deref(), Some("build-skill"));
    }
}
