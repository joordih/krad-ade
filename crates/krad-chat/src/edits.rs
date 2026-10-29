use krad_domain::{line_diff, ChatEvent, EditLine, FileEdit, LineChange, ToolUseId};
use serde::Deserialize;
use serde_json::Value;

const EDIT_TOOL: &str = "Edit";
const MULTI_EDIT_TOOL: &str = "MultiEdit";
const WRITE_TOOL: &str = "Write";
const PATH_KEY: &str = "file_path";
const OLD_KEY: &str = "old_string";
const NEW_KEY: &str = "new_string";
const EDITS_KEY: &str = "edits";
const CONTENT_KEY: &str = "content";
const RESULT_POINTERS: [&str; 2] = ["/tool_use_result", "/toolUseResult"];
const CONTENT_POINTER: &str = "/message/content";
const TOOL_RESULT_BLOCK: &str = "tool_result";
const KEPT_MARK: char = ' ';
const REMOVED_MARK: char = '-';
const ADDED_MARK: char = '+';

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn gap() -> EditLine {
    EditLine {
        change: LineChange::Gap,
        text: String::new(),
        old_line: None,
        new_line: None,
    }
}

pub(crate) fn file_edit(name: &str, input: Option<&Value>) -> Option<FileEdit> {
    let input = input?;
    let path = input.get(PATH_KEY).and_then(Value::as_str)?.to_owned();
    let lines = match name {
        EDIT_TOOL => line_diff(text(input, OLD_KEY), text(input, NEW_KEY)),
        WRITE_TOOL => line_diff("", text(input, CONTENT_KEY)),
        MULTI_EDIT_TOOL => {
            let mut lines: Vec<EditLine> = Vec::new();
            for edit in input.get(EDITS_KEY).and_then(Value::as_array)? {
                if !lines.is_empty() {
                    lines.push(gap());
                }
                lines.extend(line_diff(text(edit, OLD_KEY), text(edit, NEW_KEY)));
            }
            lines
        }
        _ => return None,
    };
    (!lines.is_empty()).then_some(FileEdit { path, lines })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchedResult {
    file_path: String,
    structured_patch: Vec<Hunk>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Hunk {
    old_start: u32,
    new_start: u32,
    lines: Vec<String>,
}

fn numbered(hunk: &Hunk) -> Vec<EditLine> {
    let (mut old, mut new) = (hunk.old_start, hunk.new_start);
    hunk.lines
        .iter()
        .filter_map(|line| {
            let mut chars = line.chars();
            let mark = chars.next()?;
            let text = chars.as_str().to_owned();
            let (change, old_line, new_line) = match mark {
                KEPT_MARK => (LineChange::Kept, Some(old), Some(new)),
                REMOVED_MARK => (LineChange::Removed, Some(old), None),
                ADDED_MARK => (LineChange::Added, None, Some(new)),
                _ => return None,
            };
            old += u32::from(old_line.is_some());
            new += u32::from(new_line.is_some());
            Some(EditLine {
                change,
                text,
                old_line,
                new_line,
            })
        })
        .collect()
}

fn answered_tool(value: &Value) -> Option<ToolUseId> {
    value
        .pointer(CONTENT_POINTER)?
        .as_array()?
        .iter()
        .find(|block| block.get("type").and_then(Value::as_str) == Some(TOOL_RESULT_BLOCK))?
        .get("tool_use_id")?
        .as_str()
        .map(ToolUseId::from)
}

pub(crate) fn patched_edit(value: &Value) -> Option<ChatEvent> {
    let result = RESULT_POINTERS
        .iter()
        .find_map(|pointer| value.pointer(pointer))?;
    let result = PatchedResult::deserialize(result).ok()?;
    let mut lines: Vec<EditLine> = Vec::new();
    for hunk in &result.structured_patch {
        if !lines.is_empty() {
            lines.push(gap());
        }
        lines.extend(numbered(hunk));
    }
    if lines.is_empty() {
        return None;
    }
    Some(ChatEvent::Edited {
        tool: answered_tool(value)?,
        edit: FileEdit {
            path: result.file_path,
            lines,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_edit_call_reads_as_the_lines_it_changes() {
        let input = json!({"file_path": "src/a.rs", "old_string": "a\nb", "new_string": "a\nc"});
        let edit = file_edit(EDIT_TOOL, Some(&input));
        assert_eq!(
            edit.as_ref().map(|edit| edit.path.as_str()),
            Some("src/a.rs")
        );
        assert_eq!(edit.map(|edit| edit.counts()), Some((1, 1)));
    }

    #[test]
    fn a_multi_edit_separates_its_hunks_and_a_read_edits_nothing() {
        let input = json!({"file_path": "x", "edits": [
            {"old_string": "a", "new_string": "b"},
            {"old_string": "c", "new_string": "d"}
        ]});
        let edit = file_edit(MULTI_EDIT_TOOL, Some(&input));
        assert_eq!(edit.map(|edit| edit.lines.len()), Some(5));
        assert_eq!(file_edit("Read", Some(&json!({"file_path": "x"}))), None);
    }

    #[test]
    fn a_tool_result_numbers_its_patch_where_it_landed() {
        let line = json!({
            "type": "user",
            "message": {"content": [{"type": "tool_result", "tool_use_id": "t1"}]},
            "toolUseResult": {
                "filePath": "src/a.rs",
                "structuredPatch": [
                    {"oldStart": 10, "oldLines": 3, "newStart": 10, "newLines": 3,
                     "lines": [" keep", "-old", "+new", "\\ No newline at end of file"]},
                    {"oldStart": 40, "oldLines": 1, "newStart": 40, "newLines": 2,
                     "lines": [" tail", "+more"]}
                ]
            }
        });
        let Some(ChatEvent::Edited { tool, edit }) = patched_edit(&line) else {
            unreachable!("an edit result reads as an edit");
        };
        assert_eq!(tool, ToolUseId::from("t1"));
        let numbers: Vec<(LineChange, Option<u32>, Option<u32>)> = edit
            .lines
            .iter()
            .map(|line| (line.change, line.old_line, line.new_line))
            .collect();
        assert_eq!(
            numbers,
            [
                (LineChange::Kept, Some(10), Some(10)),
                (LineChange::Removed, Some(11), None),
                (LineChange::Added, None, Some(11)),
                (LineChange::Gap, None, None),
                (LineChange::Kept, Some(40), Some(40)),
                (LineChange::Added, None, Some(41)),
            ]
        );
    }

    #[test]
    fn a_result_without_a_patch_is_no_edit() {
        let line = json!({
            "type": "user",
            "message": {"content": [{"type": "tool_result", "tool_use_id": "t2"}]},
            "tool_use_result": {"stdout": "ok"}
        });
        assert_eq!(patched_edit(&line), None);
    }
}
