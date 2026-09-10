//! Syntax highlighting for Piton files.
//!
//! The colours come out of the language's own grammar. The text is parsed with
//! `piton-syntax`, the crate the compiler and the language server parse with,
//! and every token is put in the same class the server would put it in for its
//! semantic tokens. The two therefore cannot disagree, which is also why the
//! editor never asks the server for semantic tokens: that classification is
//! purely syntactic, so the grammar already answers it here, without a round
//! trip and without going stale between keystrokes.

use std::ops::Range;

use eframe::egui::{
    text::{ByteIndex, LayoutJob, LayoutSection},
    Color32, FontId, Stroke, TextFormat,
};
use piton_syntax::ast::{AnchorDecl, AstNode};
use piton_syntax::kind::SyntaxKind::{self, *};
use piton_syntax::SyntaxToken;

use crate::ui::theme;

/// What a token is, in the same terms the language server uses for its
/// semantic tokens.
///
/// The name on the right of each line is the server's legend entry, so the two
/// tables can be read side by side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// `anchor`, `export`, `from`, `import`, `use`, `this` … — keyword
    Keyword,
    /// A written type constraint — type
    Type,
    /// The name a declaration binds — class
    Declaration,
    /// A key on the left of a `:` — property
    Property,
    /// A name being referred to — variable
    Name,
    /// Prose, quoted strings and escapes — string
    Text,
    /// A numeric literal — number
    Number,
    /// Punctuation and operators — operator
    Operator,
    /// A `// ...` comment — comment
    Comment,
    /// A module path such as `./Toolbar` or `@piton/belay` — namespace
    Path,
    /// The keyword a declaration invents for itself — function
    Custom,
    /// `true`, `false`, `null` — enumMember
    Literal,
    /// The `@`, `$` or name in front of an interpolation — decorator
    Sigil,
    /// Anything the grammar does not colour.
    Plain,
}

impl Class {
    /// The colour this class is drawn in.
    pub fn color(self) -> Color32 {
        match self {
            Class::Keyword => theme::SYNTAX_KEYWORD,
            Class::Type => theme::SYNTAX_TYPE,
            Class::Declaration => theme::SYNTAX_DECLARATION,
            Class::Property => theme::SYNTAX_PROPERTY,
            Class::Name => theme::SYNTAX_NAME,
            Class::Text => theme::SYNTAX_TEXT,
            Class::Number => theme::SYNTAX_NUMBER,
            Class::Operator => theme::SYNTAX_OPERATOR,
            Class::Comment => theme::SYNTAX_COMMENT,
            Class::Path => theme::SYNTAX_PATH,
            Class::Custom => theme::SYNTAX_CUSTOM,
            Class::Literal => theme::SYNTAX_LITERAL,
            Class::Sigil => theme::SYNTAX_SIGIL,
            Class::Plain => theme::TEXT,
        }
    }
}

/// One coloured run of the file, as a byte range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub class: Class,
}

/// A range the editor wants marked, such as the range a problem is on.
#[derive(Debug, Clone)]
pub struct Mark {
    pub range: Range<usize>,
    pub color: Color32,
}

/// Classify every token in a Piton file.
pub fn spans(text: &str) -> Vec<Span> {
    let parsed = piton_syntax::parse(text);
    let mut spans: Vec<Span> = Vec::new();
    for element in parsed.syntax().descendants_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };
        let class = classify(&token);
        if class == Class::Plain {
            continue;
        }
        let range = token.text_range();
        let range = usize::from(range.start())..usize::from(range.end());
        // Runs of the same class next to each other are one run to draw.
        match spans.last_mut() {
            Some(last) if last.class == class && last.range.end == range.start => {
                last.range.end = range.end;
            }
            _ => spans.push(Span { range, class }),
        }
    }
    spans
}

/// Put one token in its class, reading its parent for identifiers.
fn classify(token: &SyntaxToken) -> Class {
    match token.kind() {
        COMMENT => Class::Comment,
        ANCHOR_KW | ABSTRACT_KW | EXTENDS_KW | AS_KW | EXPORT_KW | FROM_KW | IMPORT_KW
        | USE_KW | THIS_KW | SELF_KW | SUPER_KW => Class::Keyword,
        TRUE_KW | FALSE_KW | NULL_KW => Class::Literal,
        NUMBER => Class::Number,
        QUOTED_STRING | TEXT | ESCAPE => Class::Text,
        PATH => Class::Path,
        SIGIL => Class::Sigil,
        COLON | COLON2 | COMMA | DOT | DASH | STAR | PLUS | PLUS2 | MINUS | SLASH | PERCENT
        | EQ2 | BANG_EQ | GT | LT | GT_EQ | LT_EQ | AMP2 | PIPE2 | QUESTION | L_BRACE
        | R_BRACE | L_BRACK | R_BRACK | L_PAREN | R_PAREN => Class::Operator,
        ERROR_TOKEN => Class::Plain,
        IDENT => match token.parent() {
            Some(parent) => identifier(token, parent.kind(), parent),
            None => Class::Name,
        },
        _ => Class::Plain,
    }
}

fn identifier(
    token: &SyntaxToken,
    kind: SyntaxKind,
    parent: piton_syntax::SyntaxNode,
) -> Class {
    match kind {
        // A declaration written with a keyword of its own, e.g.
        // `build-skill BuildApplication:`, holds both the keyword and the name.
        ANCHOR_DECL => match AnchorDecl::cast(parent) {
            Some(declaration)
                if declaration.keyword_token().map(|it| it.text_range())
                    == Some(token.text_range()) =>
            {
                Class::Custom
            }
            _ => Class::Declaration,
        },
        AS_CLAUSE => Class::Custom,
        EXTENDS_CLAUSE => Class::Declaration,
        TYPE_REF => Class::Type,
        PROPERTY | VAR_DECL => Class::Property,
        _ => Class::Name,
    }
}

/// Lays out highlighted text, holding on to the last parse.
///
/// The layouter runs every frame while the sidebar is open; the text only
/// changes when it is typed in, so the classification is kept until it does.
#[derive(Default)]
pub struct Highlighter {
    cached: Option<(u64, Vec<Span>)>,
}

impl Highlighter {
    /// The classification of `text`, parsing it only when it has changed.
    pub fn spans(&mut self, text: &str) -> &[Span] {
        let key = hash(text);
        match &self.cached {
            Some((cached, _)) if *cached == key => {}
            _ => self.cached = Some((key, spans(text))),
        }
        &self.cached.as_ref().expect("just filled in").1
    }

    /// Lay `text` out, coloured by class and underlined where it is marked.
    pub fn job(&mut self, text: &str, font: FontId, wrap_width: f32, marks: &[Mark]) -> LayoutJob {
        let spans = self.spans(text).to_vec();
        let mut job = LayoutJob {
            text: text.to_string(),
            wrap: eframe::egui::text::TextWrapping {
                max_width: wrap_width,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut cursor = 0usize;
        let section = |range: Range<usize>, class: Class| LayoutSection {
            leading_space: 0.0,
            byte_range: ByteIndex(range.start)..ByteIndex(range.end),
            format: format(&font, class, &underline(marks, &range)),
        };
        for span in &spans {
            if span.range.start > cursor {
                job.sections
                    .push(section(cursor..span.range.start, Class::Plain));
            }
            job.sections.push(section(span.range.clone(), span.class));
            cursor = span.range.end;
        }
        if cursor < text.len() {
            job.sections.push(section(cursor..text.len(), Class::Plain));
        }
        // A section per mark boundary would be exact; splitting a run that a
        // mark only partly covers is not worth the extra sections, so a run is
        // underlined when a mark reaches into it.
        job
    }
}

/// The colour of the mark covering `range`, if one does.
fn underline(marks: &[Mark], range: &Range<usize>) -> Option<Color32> {
    marks
        .iter()
        .find(|mark| mark.range.start < range.end && range.start < mark.range.end)
        .map(|mark| mark.color)
}

fn format(font: &FontId, class: Class, underline: &Option<Color32>) -> TextFormat {
    TextFormat {
        font_id: font.clone(),
        color: class.color(),
        underline: match underline {
            Some(color) => Stroke::new(1.0, *color),
            None => Stroke::NONE,
        },
        ..Default::default()
    }
}

fn hash(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The class the text `needle` was given, from its first occurrence.
    fn class_of(source: &str, needle: &str) -> Option<Class> {
        let at = source.find(needle).expect("the needle is in the source");
        spans(source)
            .into_iter()
            .find(|span| span.range.start <= at && at < span.range.end)
            .map(|span| span.class)
    }

    #[test]
    fn keywords_names_and_properties_are_told_apart() {
        let source = "export anchor Stack:\n    language: Rust\n";

        assert_eq!(class_of(source, "export"), Some(Class::Keyword));
        assert_eq!(class_of(source, "anchor"), Some(Class::Keyword));
        assert_eq!(class_of(source, "Stack"), Some(Class::Declaration));
        assert_eq!(class_of(source, "language"), Some(Class::Property));
        assert_eq!(class_of(source, "Rust"), Some(Class::Text));
    }

    #[test]
    fn a_declaration_that_invents_a_keyword_keeps_them_apart() {
        let source = "export build-skill BuildApplication:\n    scope: Application\n";

        assert_eq!(
            class_of(source, "build-skill"),
            Some(Class::Custom),
            "the keyword the declaration invented"
        );
        assert_eq!(
            class_of(source, "BuildApplication"),
            Some(Class::Declaration),
            "the name it binds"
        );
    }

    #[test]
    fn imports_colour_their_paths_and_names() {
        let source = "from /concept/Application import Application Concept\nuse @piton/belay\n";

        assert_eq!(class_of(source, "from"), Some(Class::Keyword));
        assert_eq!(class_of(source, "/concept/Application"), Some(Class::Path));
        assert_eq!(class_of(source, "@piton/belay"), Some(Class::Path));
        assert_eq!(class_of(source, "Concept"), Some(Class::Name));
    }

    #[test]
    fn comments_numbers_literals_and_interpolations_are_coloured() {
        let source = "anchor A:\n    // a note\n    count: 42\n    on: true\n    link: @{Other}\n";

        assert_eq!(class_of(source, "// a note"), Some(Class::Comment));
        assert_eq!(class_of(source, "42"), Some(Class::Number));
        assert_eq!(class_of(source, "true"), Some(Class::Literal));
        assert_eq!(class_of(source, "@"), Some(Class::Sigil));
        assert_eq!(class_of(source, "Other"), Some(Class::Name));
    }

    #[test]
    fn a_type_constraint_is_coloured_as_a_type() {
        let source = "abstract anchor Skill:\n    scope:: string\n";

        assert_eq!(class_of(source, "abstract"), Some(Class::Keyword));
        assert_eq!(class_of(source, "string"), Some(Class::Type));
    }

    #[test]
    fn the_spans_cover_the_file_in_order_and_nothing_else() {
        let source = "export anchor Stack:\n    language: Rust\n";
        let spans = spans(source);

        let mut end = 0;
        for span in &spans {
            assert!(span.range.start >= end, "the spans run in order");
            assert!(span.range.end <= source.len(), "no span runs off the end");
            end = span.range.end;
        }
        assert!(
            source.is_char_boundary(spans[0].range.start),
            "the spans start on character boundaries"
        );
    }

    #[test]
    fn text_that_is_not_piton_at_all_is_still_laid_out() {
        // The editor opens whatever the node stands for; half typed and plainly
        // broken files have to lay out like any other.
        for source in ["", "\n\n", "}{ nonsense ::", "anchor", "a: ${unclosed"] {
            let spans = spans(source);
            for span in spans {
                assert!(span.range.end <= source.len());
            }
        }
    }

    #[test]
    fn the_classification_is_kept_until_the_text_changes() {
        let mut highlighter = Highlighter::default();
        let first = highlighter.spans("anchor A:\n").to_vec();
        assert_eq!(highlighter.spans("anchor A:\n"), first.as_slice());

        let changed = highlighter.spans("anchor Bee:\n").to_vec();
        assert_ne!(changed, first, "new text is classified afresh");
    }

    #[test]
    fn a_marked_range_is_underlined_where_it_falls() {
        let mut highlighter = Highlighter::default();
        let source = "anchor A:\n    key: value\n";
        let at = source.find("value").unwrap();
        let marks = vec![Mark {
            range: at..at + 5,
            color: theme::DANGER,
        }];

        let job = highlighter.job(source, FontId::monospace(12.0), 400.0, &marks);

        let underlined: Vec<_> = job
            .sections
            .iter()
            .filter(|section| section.format.underline != Stroke::NONE)
            .collect();
        assert!(!underlined.is_empty(), "the marked range is underlined");
        for section in &underlined {
            assert!(
                section.byte_range.start.0 < at + 5 && at < section.byte_range.end.0,
                "only the marked range is underlined"
            );
            assert_eq!(section.format.underline.color, theme::DANGER);
        }
    }

    #[test]
    fn the_laid_out_sections_cover_the_whole_text() {
        let mut highlighter = Highlighter::default();
        let source = "export anchor Stack:\n    language: Rust\n\n    // trailing\n";
        let job = highlighter.job(source, FontId::monospace(12.0), 400.0, &[]);

        assert_eq!(job.text, source);
        let mut covered = 0;
        for section in &job.sections {
            assert_eq!(section.byte_range.start.0, covered, "no gaps between sections");
            covered = section.byte_range.end.0;
        }
        assert_eq!(covered, source.len(), "the sections reach the end");
    }
}
