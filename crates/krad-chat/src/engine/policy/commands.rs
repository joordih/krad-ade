const SINGLE_QUOTE: char = '\'';
const DOUBLE_QUOTE: char = '"';
const BACKSLASH: char = '\\';
const BACKTICK: char = '`';
const DOLLAR: char = '$';
const OPEN_PAREN: char = '(';
const CLOSE_PAREN: char = ')';
const PIPE: char = '|';
const AMPERSAND: char = '&';
const SEMICOLON: char = ';';
const NEWLINE: char = '\n';
const REDIRECT_OUT: char = '>';
const REDIRECT_IN: char = '<';
const PATH_SEPARATOR: char = '/';
const ASSIGNMENT: char = '=';
const FLAG_MARK: char = '-';
const LONG_FLAG_MARK: &str = "--";

const SAFE_PROGRAMS: [&str; 24] = [
    "ls", "cat", "head", "tail", "wc", "pwd", "echo", "which", "file", "stat", "du", "df", "tree",
    "grep", "basename", "dirname", "realpath", "date", "whoami", "uname", "nl", "uniq", "cut",
    "true",
];
const RIPGREP: &str = "rg";
const RIPGREP_UNSAFE: [&str; 4] = ["--pre", "--hostname-bin", "--search-zip", "-z"];
const FIND: &str = "find";
const FIND_UNSAFE: [&str; 9] = [
    "-exec", "-execdir", "-ok", "-okdir", "-delete", "-fls", "-fprint", "-fprint0", "-fprintf",
];
const SED: &str = "sed";
const SED_QUIET: &str = "-n";
const SED_PRINT: char = 'p';
const SED_RANGE: char = ',';
const GIT: &str = "git";
const GIT_READS: [&str; 8] = [
    "status",
    "log",
    "diff",
    "show",
    "rev-parse",
    "ls-files",
    "blame",
    "grep",
];
const GIT_UNSAFE_FLAGS: [&str; 2] = ["--output", "--ext-diff"];
const GIT_BRANCH: &str = "branch";
const GIT_BRANCH_LISTING: [&str; 8] = [
    "--list",
    "-l",
    "-a",
    "--all",
    "-r",
    "--show-current",
    "-v",
    "-vv",
];
const CARGO: &str = "cargo";
const CARGO_READS: [&str; 4] = ["tree", "metadata", "version", "--version"];

const ELEVATING: [&str; 3] = ["sudo", "su", "doas"];
const DESTRUCTIVE: [&str; 4] = ["dd", "shutdown", "reboot", "halt"];
const FORMATTER_PREFIX: &str = "mkfs";
const SHELLS: [&str; 7] = ["sh", "bash", "zsh", "fish", "dash", "ksh", "csh"];
const REMOVE: &str = "rm";
const RECURSIVE_FLAGS: [char; 2] = ['r', 'R'];
const FORCE_FLAG: char = 'f';
const LONG_RECURSIVE: &str = "--recursive";
const LONG_FORCE: &str = "--force";
const PUSH: &str = "push";
const FORCE_PUSH_FLAGS: [&str; 2] = ["-f", "--force"];
const FORCE_PUSH_PREFIX: &str = "--force";
const RESET: &str = "reset";
const HARD_FLAG: &str = "--hard";
const CLEAN: &str = "clean";
const PERMISSION_PROGRAMS: [&str; 2] = ["chmod", "chown"];
const RECURSIVE_SHORT: &str = "-R";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Segment {
    pub(super) words: Vec<String>,
    pub(super) piped: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Construct {
    Redirect,
    Substitution,
    Subshell,
    Background,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Parsed {
    pub(super) segments: Vec<Segment>,
    constructs: Vec<Construct>,
}

impl Parsed {
    pub(super) fn is_plain(&self) -> bool {
        self.constructs.is_empty()
    }

    fn note(&mut self, construct: Construct) {
        if !self.constructs.contains(&construct) {
            self.constructs.push(construct);
        }
    }
}

struct Splitter {
    parsed: Parsed,
    word: Option<String>,
    words: Vec<String>,
    piped: bool,
}

impl Splitter {
    fn push_char(&mut self, ch: char) {
        self.word.get_or_insert_with(String::new).push(ch);
    }

    fn end_word(&mut self) {
        if let Some(word) = self.word.take() {
            self.words.push(word);
        }
    }

    fn end_segment(&mut self, next_piped: bool) {
        self.end_word();
        let words = std::mem::take(&mut self.words);
        if !words.is_empty() {
            self.parsed.segments.push(Segment {
                words,
                piped: self.piped,
            });
        }
        self.piped = next_piped;
    }
}

pub(super) fn parse(command: &str) -> Parsed {
    let mut splitter = Splitter {
        parsed: Parsed::default(),
        word: None,
        words: Vec::new(),
        piped: false,
    };
    let mut chars = command.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            SINGLE_QUOTE => {
                splitter.word.get_or_insert_with(String::new);
                for quoted in chars.by_ref() {
                    if quoted == SINGLE_QUOTE {
                        break;
                    }
                    splitter.push_char(quoted);
                }
            }
            DOUBLE_QUOTE => {
                splitter.word.get_or_insert_with(String::new);
                while let Some(quoted) = chars.next() {
                    match quoted {
                        DOUBLE_QUOTE => break,
                        BACKTICK => splitter.parsed.note(Construct::Substitution),
                        DOLLAR if chars.peek() == Some(&OPEN_PAREN) => {
                            splitter.parsed.note(Construct::Substitution);
                        }
                        BACKSLASH => {
                            if let Some(escaped) = chars.next() {
                                splitter.push_char(escaped);
                            }
                        }
                        other => splitter.push_char(other),
                    }
                }
            }
            BACKSLASH => {
                if let Some(escaped) = chars.next() {
                    splitter.push_char(escaped);
                }
            }
            BACKTICK => splitter.parsed.note(Construct::Substitution),
            DOLLAR if chars.peek() == Some(&OPEN_PAREN) => {
                splitter.parsed.note(Construct::Substitution);
            }
            OPEN_PAREN | CLOSE_PAREN => splitter.parsed.note(Construct::Subshell),
            REDIRECT_OUT | REDIRECT_IN => {
                splitter.parsed.note(Construct::Redirect);
                splitter.end_word();
            }
            PIPE => {
                let or = chars.next_if_eq(&PIPE).is_some();
                splitter.end_segment(!or);
            }
            AMPERSAND => {
                if chars.next_if_eq(&AMPERSAND).is_none() {
                    splitter.parsed.note(Construct::Background);
                }
                splitter.end_segment(false);
            }
            SEMICOLON | NEWLINE => splitter.end_segment(false),
            space if space.is_whitespace() => splitter.end_word(),
            other => splitter.push_char(other),
        }
    }
    splitter.end_segment(false);
    splitter.parsed
}

fn has_any(arguments: &[String], flags: &[&str]) -> bool {
    arguments.iter().any(|argument| {
        flags.iter().any(|flag| {
            argument == flag
                || argument
                    .strip_prefix(flag)
                    .is_some_and(|rest| rest.starts_with(ASSIGNMENT))
        })
    })
}

fn sed_prints_lines(arguments: &[String]) -> bool {
    let [quiet, script, ..] = arguments else {
        return false;
    };
    let Some(range) = script.strip_suffix(SED_PRINT) else {
        return false;
    };
    quiet == SED_QUIET
        && !range.is_empty()
        && range
            .split(SED_RANGE)
            .all(|bound| !bound.is_empty() && bound.chars().all(|ch| ch.is_ascii_digit()))
        && range.split(SED_RANGE).count() <= 2
}

fn git_reads(arguments: &[String]) -> bool {
    let Some((sub, rest)) = arguments.split_first() else {
        return false;
    };
    if sub == GIT_BRANCH {
        return rest
            .iter()
            .all(|argument| GIT_BRANCH_LISTING.contains(&argument.as_str()));
    }
    GIT_READS.contains(&sub.as_str()) && !has_any(rest, &GIT_UNSAFE_FLAGS)
}

pub(super) fn segment_is_safe(words: &[String]) -> bool {
    let Some((program, arguments)) = words.split_first() else {
        return false;
    };
    if program.contains(PATH_SEPARATOR) || program.contains(ASSIGNMENT) {
        return false;
    }
    match program.as_str() {
        RIPGREP => !has_any(arguments, &RIPGREP_UNSAFE),
        FIND => !has_any(arguments, &FIND_UNSAFE),
        SED => sed_prints_lines(arguments),
        GIT => git_reads(arguments),
        CARGO => arguments
            .first()
            .is_some_and(|sub| CARGO_READS.contains(&sub.as_str())),
        other => SAFE_PROGRAMS.contains(&other),
    }
}

pub(super) fn is_known_safe(command: &str) -> bool {
    let parsed = parse(command);
    parsed.is_plain()
        && !parsed.segments.is_empty()
        && parsed
            .segments
            .iter()
            .all(|segment| segment_is_safe(&segment.words))
}

fn short_flags(arguments: &[String]) -> String {
    arguments
        .iter()
        .filter(|argument| argument.starts_with(FLAG_MARK) && !argument.starts_with(LONG_FLAG_MARK))
        .flat_map(|argument| argument.chars().skip(1))
        .collect()
}

fn removes_by_force(arguments: &[String]) -> bool {
    let flags = short_flags(arguments);
    let recursive = flags.contains(RECURSIVE_FLAGS) || has_any(arguments, &[LONG_RECURSIVE]);
    let force = flags.contains(FORCE_FLAG) || has_any(arguments, &[LONG_FORCE]);
    recursive && force
}

fn git_rewrites(arguments: &[String]) -> bool {
    let Some((sub, rest)) = arguments.split_first() else {
        return false;
    };
    match sub.as_str() {
        PUSH => {
            has_any(rest, &FORCE_PUSH_FLAGS)
                || rest
                    .iter()
                    .any(|argument| argument.starts_with(FORCE_PUSH_PREFIX))
        }
        RESET => rest.iter().any(|argument| argument == HARD_FLAG),
        CLEAN => short_flags(rest).contains(FORCE_FLAG) || has_any(rest, &[LONG_FORCE]),
        _ => false,
    }
}

fn segment_is_dangerous(segment: &Segment) -> bool {
    let Some((program, arguments)) = segment.words.split_first() else {
        return false;
    };
    let name = program.rsplit(PATH_SEPARATOR).next().unwrap_or(program);
    if segment.piped && SHELLS.contains(&name) {
        return true;
    }
    match name {
        REMOVE => removes_by_force(arguments),
        GIT => git_rewrites(arguments),
        name if PERMISSION_PROGRAMS.contains(&name) => {
            arguments.iter().any(|argument| argument == RECURSIVE_SHORT)
                || has_any(arguments, &[LONG_RECURSIVE])
        }
        name => {
            ELEVATING.contains(&name)
                || DESTRUCTIVE.contains(&name)
                || name.starts_with(FORMATTER_PREFIX)
        }
    }
}

pub(super) fn is_dangerous(command: &str) -> bool {
    parse(command).segments.iter().any(segment_is_dangerous)
}

pub(super) fn segment_key(segment: &Segment) -> Option<String> {
    let mut words = segment.words.iter();
    let program = words.next()?;
    Some(
        match words.next().filter(|word| !word.starts_with(FLAG_MARK)) {
            Some(sub) => format!("{program} {sub}"),
            None => program.clone(),
        },
    )
}

#[cfg(test)]
mod tests;
