const BACKTICK: char = '`';
const TILDE: char = '~';
const FENCE_MIN: usize = 3;
const FENCE_INDENT_MAX: usize = 3;
const INDENT: char = ' ';
const NEWLINE: char = '\n';
const CARRIAGE_RETURN: char = '\r';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AnswerBlock<'a> {
    Prose(&'a str),
    Code { source: &'a str, code: &'a str },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fence {
    mark: char,
    len: usize,
}

fn without_line_end(line: &str) -> &str {
    line.trim_end_matches([NEWLINE, CARRIAGE_RETURN])
}

fn mark_run(line: &str, mark: char) -> usize {
    line.chars().take_while(|&c| c == mark).count()
}

fn unindented(line: &str) -> Option<&str> {
    let rest = line.trim_start_matches(INDENT);
    (line.len() - rest.len() <= FENCE_INDENT_MAX).then_some(rest)
}

fn opening_fence(line: &str) -> Option<Fence> {
    let line = unindented(without_line_end(line))?;
    let mark = line
        .chars()
        .next()
        .filter(|&c| c == BACKTICK || c == TILDE)?;
    let len = mark_run(line, mark);
    if len < FENCE_MIN {
        return None;
    }
    let info = &line[len * mark.len_utf8()..];
    if mark == BACKTICK && info.contains(BACKTICK) {
        return None;
    }
    Some(Fence { mark, len })
}

fn closes(fence: Fence, line: &str) -> bool {
    let Some(unindented) = unindented(without_line_end(line)) else {
        return false;
    };
    let len = mark_run(unindented, fence.mark);
    len >= fence.len && unindented[len * fence.mark.len_utf8()..].trim().is_empty()
}

fn push_prose<'a>(blocks: &mut Vec<AnswerBlock<'a>>, prose: &'a str) {
    if !prose.trim().is_empty() {
        blocks.push(AnswerBlock::Prose(prose));
    }
}

fn code_block(text: &str, start: usize, code: (usize, usize), end: usize) -> AnswerBlock<'_> {
    AnswerBlock::Code {
        source: without_line_end(&text[start..end]),
        code: without_line_end(&text[code.0..code.1]),
    }
}

pub(super) fn answer_blocks(text: &str) -> Vec<AnswerBlock<'_>> {
    let mut blocks = Vec::new();
    let mut prose_start = 0;
    let mut open: Option<(Fence, usize, usize)> = None;
    let mut offset = 0;
    for line in text.split_inclusive(NEWLINE) {
        let line_start = offset;
        offset += line.len();
        match open {
            None => {
                if let Some(fence) = opening_fence(line) {
                    push_prose(&mut blocks, &text[prose_start..line_start]);
                    open = Some((fence, line_start, offset));
                }
            }
            Some((fence, start, code_start)) if closes(fence, line) => {
                blocks.push(code_block(text, start, (code_start, line_start), offset));
                prose_start = offset;
                open = None;
            }
            Some(_) => {}
        }
    }
    match open {
        Some((_, start, code_start)) => {
            blocks.push(code_block(
                text,
                start,
                (code_start, text.len()),
                text.len(),
            ));
        }
        None => push_prose(&mut blocks, &text[prose_start..]),
    }
    blocks
}

pub(super) fn code_of(text: &str, block: usize) -> Option<&str> {
    match answer_blocks(text).get(block)? {
        AnswerBlock::Code { code, .. } => Some(code),
        AnswerBlock::Prose(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prose_without_fences_stays_one_block() {
        let text = "Hello\n\nworld";
        assert_eq!(answer_blocks(text), vec![AnswerBlock::Prose(text)]);
    }

    #[test]
    fn a_fenced_block_splits_the_answer_and_keeps_its_code() {
        let text = "Run this:\n```sh\nls -la\ncd ..\n```\nDone.";
        assert_eq!(
            answer_blocks(text),
            vec![
                AnswerBlock::Prose("Run this:\n"),
                AnswerBlock::Code {
                    source: "```sh\nls -la\ncd ..\n```",
                    code: "ls -la\ncd ..",
                },
                AnswerBlock::Prose("Done."),
            ]
        );
    }

    #[test]
    fn a_fence_closes_only_on_its_own_mark_and_length() {
        let text = "````md\n```rust\nfn main() {}\n```\n````";
        assert_eq!(
            answer_blocks(text),
            vec![AnswerBlock::Code {
                source: text,
                code: "```rust\nfn main() {}\n```",
            }]
        );
    }

    #[test]
    fn an_unclosed_fence_runs_to_the_end_while_streaming() {
        let text = "Here:\n~~~\npartial\n";
        assert_eq!(
            answer_blocks(text),
            vec![
                AnswerBlock::Prose("Here:\n"),
                AnswerBlock::Code {
                    source: "~~~\npartial",
                    code: "partial",
                },
            ]
        );
    }

    #[test]
    fn inline_backticks_and_fences_indented_four_spaces_are_not_blocks() {
        let text = "Use ```inline``` code\n    ```\n    nested\n    ```";
        assert_eq!(answer_blocks(text), vec![AnswerBlock::Prose(text)]);
    }

    #[test]
    fn a_fence_indented_up_to_three_spaces_opens_a_block() {
        for indent in [" ", "  ", "   "] {
            let text = format!("Steps:\n{indent}```sh\n{indent}cargo test\n{indent}```\n");
            assert_eq!(
                code_of(&text, 1),
                Some(format!("{indent}cargo test").as_str()),
                "{indent:?}"
            );
        }
    }

    #[test]
    fn an_empty_fence_has_empty_code() {
        assert_eq!(code_of("```\n```", 0), Some(""));
        assert_eq!(code_of("text", 0), None);
    }

    #[test]
    fn crlf_lines_keep_their_code_clean() {
        assert_eq!(code_of("```\r\nx = 1\r\n```\r\n", 0), Some("x = 1"));
    }
}
