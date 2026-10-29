use super::{KEY_GENERATED_BY, KEY_MODEL, KEY_SOURCES, NOTE_EXTENSION, VAULT_DIR};
use crate::model::ModelId;

const SLUG_SEPARATOR: char = '-';
const FENCE: &str = "---";
const PATH_SEPARATOR: char = '/';

#[must_use]
pub fn slug(title: &str) -> String {
    let words: Vec<String> = title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect();
    words.join(&SLUG_SEPARATOR.to_string())
}

fn folder_path(folder: &str) -> String {
    let folder = folder.trim().trim_matches(PATH_SEPARATOR);
    if folder.is_empty() {
        VAULT_DIR.to_owned()
    } else {
        format!("{VAULT_DIR}{PATH_SEPARATOR}{folder}")
    }
}

#[must_use]
pub fn docs_prompt(
    notes: &[String],
    folder: &str,
    sources: &[String],
    keep: &[String],
    harness: &str,
    model: Option<&ModelId>,
) -> String {
    let folder = folder_path(folder);
    let files: Vec<String> = notes
        .iter()
        .map(|note| format!("{folder}/{}.{NOTE_EXTENSION} ({note})", slug(note)))
        .collect();
    let model = model.map_or_else(String::new, |model| format!("\n{KEY_MODEL}: {model}"));
    let keep = if keep.is_empty() {
        String::new()
    } else {
        format!(
            " Do not change these notes, a person edited them: {}.",
            keep.join(", ")
        )
    };
    format!(
        "Write developer documentation for this repository as Markdown notes: {}. \
         Read {}. Check what those documents say against the code; where they disagree, \
         follow the code and add a `> [!WARNING]` callout naming the document. Start every \
         note with this front-matter, then a # title:\n\
         {FENCE}\n{KEY_GENERATED_BY}: {harness}{model}\n{KEY_SOURCES}: [{}]\n{FENCE}\n\
         Use headings, short paragraphs, tables, fenced code, `> [!NOTE]` callouts and \
         standard Markdown links ([text](other-note.md)) between the notes. Write only those \
         files.{keep}",
        files.join(", "),
        sources.join(", "),
        sources.join(", "),
    )
}

#[cfg(test)]
mod tests {}
