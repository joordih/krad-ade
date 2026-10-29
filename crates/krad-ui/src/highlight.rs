use std::ops::Range;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Keyword,
    String,
    Comment,
    Number,
    TypeName,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Span {
    pub(crate) range: Range<usize>,
    pub(crate) kind: TokenKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Language {
    Rust,
    TypeScript,
    Python,
    Go,
    CFamily,
    Shell,
    Config,
    Json,
    Plain,
}

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while",
];
const TYPESCRIPT_KEYWORDS: &[&str] = &[
    "as",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "from",
    "function",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "of",
    "private",
    "protected",
    "public",
    "readonly",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "type",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "yield",
];
const PYTHON_KEYWORDS: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif",
    "else", "except", "False", "finally", "for", "from", "global", "if", "import", "in", "is",
    "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "self", "True", "try",
    "while", "with", "yield",
];
const GO_KEYWORDS: &[&str] = &[
    "break",
    "case",
    "chan",
    "const",
    "continue",
    "default",
    "defer",
    "else",
    "fallthrough",
    "false",
    "for",
    "func",
    "go",
    "goto",
    "if",
    "import",
    "interface",
    "map",
    "nil",
    "package",
    "range",
    "return",
    "select",
    "struct",
    "switch",
    "true",
    "type",
    "var",
];
const C_KEYWORDS: &[&str] = &[
    "auto",
    "bool",
    "break",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "default",
    "delete",
    "do",
    "double",
    "else",
    "enum",
    "extern",
    "false",
    "final",
    "float",
    "for",
    "if",
    "import",
    "int",
    "long",
    "namespace",
    "new",
    "null",
    "nullptr",
    "override",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "signed",
    "sizeof",
    "static",
    "struct",
    "switch",
    "template",
    "this",
    "throw",
    "true",
    "try",
    "typedef",
    "union",
    "unsigned",
    "using",
    "virtual",
    "void",
    "volatile",
    "while",
];
const SHELL_KEYWORDS: &[&str] = &[
    "case", "do", "done", "elif", "else", "esac", "export", "fi", "for", "function", "if", "in",
    "local", "return", "then", "until", "while",
];
const CONFIG_KEYWORDS: &[&str] = &["false", "true", "null"];

const SINGLE_BYTE: usize = 1;
const MIN_SEQUENCE_BYTES: u32 = 2;
const MAX_SEQUENCE_BYTES: u32 = 4;
const LINE_COMMENT_SLASHES: &str = "//";
const LINE_COMMENT_HASH: &str = "#";
const BLOCK_COMMENT_OPEN: &str = "/*";
const BLOCK_COMMENT_CLOSE: &str = "*/";
const ESCAPE: u8 = b'\\';

impl Language {
    pub(crate) fn from_path(path: &Path) -> Self {
        let extension = path
            .extension()
            .map(|extension| extension.to_string_lossy().to_ascii_lowercase());
        match extension.as_deref() {
            Some("rs") => Self::Rust,
            Some("ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs") => Self::TypeScript,
            Some("py" | "pyi") => Self::Python,
            Some("go") => Self::Go,
            Some(
                "c" | "h" | "cc" | "cpp" | "hpp" | "cs" | "java" | "kt" | "swift" | "scala" | "css"
                | "scss",
            ) => Self::CFamily,
            Some("sh" | "bash" | "zsh" | "fish" | "ps1") => Self::Shell,
            Some("toml" | "yaml" | "yml" | "ini" | "cfg" | "conf" | "env") => Self::Config,
            Some("json" | "jsonc") => Self::Json,
            _ => Self::Plain,
        }
    }

    const fn keywords(self) -> &'static [&'static str] {
        match self {
            Self::Rust => RUST_KEYWORDS,
            Self::TypeScript => TYPESCRIPT_KEYWORDS,
            Self::Python => PYTHON_KEYWORDS,
            Self::Go => GO_KEYWORDS,
            Self::CFamily => C_KEYWORDS,
            Self::Shell => SHELL_KEYWORDS,
            Self::Config | Self::Json => CONFIG_KEYWORDS,
            Self::Plain => &[],
        }
    }

    const fn line_comment(self) -> Option<&'static str> {
        match self {
            Self::Rust | Self::TypeScript | Self::Go | Self::CFamily => Some(LINE_COMMENT_SLASHES),
            Self::Python | Self::Shell | Self::Config => Some(LINE_COMMENT_HASH),
            Self::Json | Self::Plain => None,
        }
    }

    const fn has_block_comments(self) -> bool {
        matches!(
            self,
            Self::Rust | Self::TypeScript | Self::Go | Self::CFamily
        )
    }

    const fn capitalised_types(self) -> bool {
        matches!(
            self,
            Self::Rust | Self::TypeScript | Self::Go | Self::CFamily | Self::Python
        )
    }

    const fn quotes(self) -> &'static [u8] {
        match self {
            Self::Rust | Self::Json => b"\"",
            Self::TypeScript => b"\"'`",
            Self::Plain => b"",
            Self::Python | Self::Go | Self::CFamily | Self::Shell | Self::Config => b"\"'",
        }
    }
}

pub(crate) fn highlight_lines(text: &str, language: Language) -> Vec<Vec<Span>> {
    let mut in_block_comment = false;
    text.lines()
        .map(|line| highlight_line(line, language, &mut in_block_comment))
        .collect()
}

fn highlight_line(line: &str, language: Language, in_block_comment: &mut bool) -> Vec<Span> {
    let bytes = line.as_bytes();
    let mut spans = Vec::new();
    let mut index = 0;
    if *in_block_comment {
        let end = find_from(line, 0, BLOCK_COMMENT_CLOSE)
            .map_or(bytes.len(), |close| close + BLOCK_COMMENT_CLOSE.len());
        *in_block_comment = end == bytes.len() && !line.ends_with(BLOCK_COMMENT_CLOSE);
        push(&mut spans, 0..end, TokenKind::Comment);
        index = end;
    }
    while index < bytes.len() {
        let rest = &line[index..];
        if language
            .line_comment()
            .is_some_and(|marker| rest.starts_with(marker))
        {
            push(&mut spans, index..bytes.len(), TokenKind::Comment);
            break;
        }
        if language.has_block_comments() && rest.starts_with(BLOCK_COMMENT_OPEN) {
            let close = find_from(line, index + BLOCK_COMMENT_OPEN.len(), BLOCK_COMMENT_CLOSE);
            let end = close.map_or(bytes.len(), |close| close + BLOCK_COMMENT_CLOSE.len());
            *in_block_comment = close.is_none();
            push(&mut spans, index..end, TokenKind::Comment);
            index = end;
            continue;
        }
        let byte = bytes[index];
        if language.quotes().contains(&byte) {
            let end = string_end(bytes, index);
            push(&mut spans, index..end, TokenKind::String);
            index = end;
        } else if byte.is_ascii_digit() {
            let end = scan(bytes, index, |byte| {
                byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'_'
            });
            push(&mut spans, index..end, TokenKind::Number);
            index = end;
        } else if is_word_start(byte) {
            let end = scan(bytes, index, is_word_byte);
            let word = &line[index..end];
            if language.keywords().contains(&word) {
                push(&mut spans, index..end, TokenKind::Keyword);
            } else if language.capitalised_types() && word.starts_with(char::is_uppercase) {
                push(&mut spans, index..end, TokenKind::TypeName);
            }
            index = end;
        } else {
            index += utf8_width(byte);
        }
    }
    spans
}

fn push(spans: &mut Vec<Span>, range: Range<usize>, kind: TokenKind) {
    if !range.is_empty() {
        spans.push(Span { range, kind });
    }
}

fn find_from(line: &str, start: usize, needle: &str) -> Option<usize> {
    line.get(start..)?.find(needle).map(|found| start + found)
}

fn string_end(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            ESCAPE => index += 2,
            byte if byte == quote => return index + 1,
            _ => index += 1,
        }
    }
    bytes.len()
}

fn scan(bytes: &[u8], start: usize, keep: impl Fn(u8) -> bool) -> usize {
    bytes[start..]
        .iter()
        .position(|byte| !keep(*byte))
        .map_or(bytes.len(), |offset| start + offset)
}

const fn is_word_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

const fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn utf8_width(byte: u8) -> usize {
    match byte.leading_ones() {
        width @ MIN_SEQUENCE_BYTES..=MAX_SEQUENCE_BYTES => {
            usize::try_from(width).unwrap_or(SINGLE_BYTE)
        }
        _ => SINGLE_BYTE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(line: &str, spans: &[Span]) -> Vec<(String, TokenKind)> {
        spans
            .iter()
            .map(|span| (line[span.range.clone()].to_owned(), span.kind))
            .collect()
    }

    #[test]
    fn rust_line_marks_keywords_types_strings_numbers_and_comments() {
        let line = r#"let path: PathBuf = "a\"b".into(); // 42 is not a number here"#;
        let spans = highlight_lines(line, Language::Rust);
        assert_eq!(
            kinds(line, &spans[0]),
            vec![
                ("let".to_owned(), TokenKind::Keyword),
                ("PathBuf".to_owned(), TokenKind::TypeName),
                (r#""a\"b""#.to_owned(), TokenKind::String),
                ("// 42 is not a number here".to_owned(), TokenKind::Comment),
            ]
        );
    }

    #[test]
    fn block_comments_carry_across_lines() {
        let text = "a /* open\nstill inside\nclose */ return 1";
        let spans = highlight_lines(text, Language::TypeScript);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            kinds(lines[1], &spans[1]),
            vec![("still inside".to_owned(), TokenKind::Comment)]
        );
        assert_eq!(
            kinds(lines[2], &spans[2]),
            vec![
                ("close */".to_owned(), TokenKind::Comment),
                ("return".to_owned(), TokenKind::Keyword),
                ("1".to_owned(), TokenKind::Number),
            ]
        );
    }

    #[test]
    fn hash_comments_and_unknown_files() {
        assert_eq!(
            Language::from_path(Path::new("Cargo.toml")),
            Language::Config
        );
        let spans = highlight_lines("key = 'v' # note", Language::Config);
        assert_eq!(spans[0].len(), 2);
        assert_eq!(highlight_lines("let it be", Language::Plain)[0], []);
    }
}
