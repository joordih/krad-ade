use super::request::CliError;
use super::verb::{LIST, MEMORY, NEW, SHARE, SHOW, UNSHARE};
use crate::protocol::{
    KraddieChatInfo, KraddieRecallInfo, KraddieSharingInfo, MemoryEntryInfo, MemoryHitInfo,
    RecallView, Request,
};
use krad_domain::Switch;

const NO_KEEPSAKES: &str = "No keepsakes yet";
const NO_DIGESTS: &str = "No chat digests yet";
const NO_HITS: &str = "No memory matches";
const KEEPSAKES_HEADING: &str = "Keepsakes";
const DIGESTS_HEADING: &str = "Chat digests";
const READS_LABEL: &str = "Reads the memory of: ";
const READS_NOBODY: &str = "Reads no other Kraddie's memory";
const DIGESTS_OFF: &str = "Automatic chat digests are off";
const LIST_SEPARATOR: &str = ", ";
const FACT_INDENT: &str = "    - ";
const COLUMN_GAP: &str = "  ";
const FACT_COUNT_SUFFIX: &str = " facts";

pub(super) fn memory_request(rest: &[String]) -> Result<Request, CliError> {
    let view = match rest.first().map(String::as_str) {
        Some(LIST) => Some(RecallView::List),
        Some(SHOW) => Some(RecallView::Show),
        _ => None,
    };
    let Some(view) = view else {
        let name = rest.first().ok_or(CliError::KraddieNeedsName(MEMORY))?;
        return Ok(Request::KraddieMemory { name: name.clone() });
    };
    let name = rest.get(1).ok_or(CliError::KraddieNeedsName(MEMORY))?;
    Ok(Request::KraddieRecall {
        name: name.clone(),
        view,
    })
}

pub(super) fn chat_request(rest: &[String]) -> Result<Request, CliError> {
    match rest {
        [new, name, ..] if new == NEW => Ok(Request::NewKraddieChat { name: name.clone() }),
        _ => Err(CliError::KraddieChatNeedsNew),
    }
}

pub(super) fn share_request(rest: &[String], switch: Switch) -> Result<Request, CliError> {
    let verb = if switch.is_on() { SHARE } else { UNSHARE };
    match rest {
        [reader, from, ..] => Ok(Request::ShareKraddieMemory {
            reader: reader.clone(),
            from: from.clone(),
            switch,
        }),
        _ => Err(CliError::KraddieShareNeedsPair(verb)),
    }
}

fn joined(lines: impl Iterator<Item = String>) -> String {
    lines.collect::<Vec<_>>().join("\n")
}

fn reads_line(reads: &[String]) -> String {
    if reads.is_empty() {
        return READS_NOBODY.to_owned();
    }
    format!("{READS_LABEL}{}", reads.join(LIST_SEPARATOR))
}

pub(super) fn render_recall(recall: &KraddieRecallInfo, view: RecallView) -> String {
    let mut out = vec![KEEPSAKES_HEADING.to_owned()];
    if recall.keepsakes.is_empty() {
        out.push(NO_KEEPSAKES.to_owned());
    }
    out.extend(
        recall
            .keepsakes
            .iter()
            .enumerate()
            .map(|(index, line)| format!("{}{COLUMN_GAP}{line}", index + 1)),
    );
    out.push(DIGESTS_HEADING.to_owned());
    if recall.digests.is_empty() {
        out.push(NO_DIGESTS.to_owned());
    }
    for digest in &recall.digests {
        match view {
            RecallView::List => out.push(format!(
                "{}{COLUMN_GAP}{}{COLUMN_GAP}{}{FACT_COUNT_SUFFIX}",
                digest.id,
                digest.chat,
                digest.facts.len()
            )),
            RecallView::Show => {
                out.push(format!("{}{COLUMN_GAP}{}", digest.id, digest.chat));
                out.extend(
                    digest
                        .facts
                        .iter()
                        .map(|fact| format!("{FACT_INDENT}{fact}")),
                );
            }
        }
    }
    out.push(reads_line(&recall.reads));
    if !recall.automatic_digests {
        out.push(DIGESTS_OFF.to_owned());
    }
    joined(out.into_iter())
}

pub(super) fn render_shared(shared: &KraddieSharingInfo) -> String {
    format!("{}: {}", shared.kraddie, reads_line(&shared.reads))
}

pub(super) fn render_chat_opened(opened: &KraddieChatInfo) -> String {
    format!(
        "Opened a chat with Kraddie {} (session {}, terminal {})",
        opened.kraddie, opened.session, opened.terminal
    )
}

pub(super) fn render_hits(hits: &[MemoryHitInfo]) -> String {
    if hits.is_empty() {
        return NO_HITS.to_owned();
    }
    joined(hits.iter().map(|hit| {
        let title = hit
            .title
            .as_ref()
            .map_or_default(|title| format!("{title}{COLUMN_GAP}"));
        format!("{}{COLUMN_GAP}{title}{}", hit.id, hit.excerpt)
    }))
}

pub(super) fn render_entry(entry: &MemoryEntryInfo) -> String {
    let title = entry
        .title
        .as_ref()
        .map_or_default(|title| format!("{COLUMN_GAP}{title}"));
    format!("{}{title}\n{}", entry.id, entry.text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::DigestInfo;

    fn words(text: &[&str]) -> Vec<String> {
        text.iter().map(|word| (*word).to_owned()).collect()
    }

    #[test]
    fn memory_list_and_show_read_the_whole_memory_and_the_old_form_stays() {
        assert_eq!(
            memory_request(&words(&["list", "owl"])),
            Ok(Request::KraddieRecall {
                name: "owl".to_owned(),
                view: RecallView::List
            })
        );
        assert_eq!(
            memory_request(&words(&["show", "owl"])),
            Ok(Request::KraddieRecall {
                name: "owl".to_owned(),
                view: RecallView::Show
            })
        );
        assert_eq!(
            memory_request(&words(&["owl"])),
            Ok(Request::KraddieMemory {
                name: "owl".to_owned()
            })
        );
        assert_eq!(
            memory_request(&words(&["show"])),
            Err(CliError::KraddieNeedsName(MEMORY))
        );
    }

    #[test]
    fn chat_and_share_requests_name_their_kraddies() {
        assert_eq!(
            chat_request(&words(&["new", "owl"])),
            Ok(Request::NewKraddieChat {
                name: "owl".to_owned()
            })
        );
        assert_eq!(
            chat_request(&words(&["owl"])),
            Err(CliError::KraddieChatNeedsNew)
        );
        assert_eq!(
            share_request(&words(&["owl", "lint"]), Switch::On),
            Ok(Request::ShareKraddieMemory {
                reader: "owl".to_owned(),
                from: "lint".to_owned(),
                switch: Switch::On
            })
        );
        assert_eq!(
            share_request(&words(&["owl"]), Switch::Off),
            Err(CliError::KraddieShareNeedsPair(UNSHARE))
        );
    }

    #[test]
    fn a_recall_lists_compactly_or_shows_every_fact() {
        let recall = KraddieRecallInfo {
            kraddie: "owl".to_owned(),
            keepsakes: words(&["Deploy on Fridays"]),
            digests: vec![DigestInfo {
                id: "kraddie/owl/digest/2".to_owned(),
                chat: "Release".to_owned(),
                facts: words(&["Tag before pushing", "CI is slow"]),
            }],
            reads: words(&["lint"]),
            automatic_digests: false,
        };
        assert_eq!(
            render_recall(&recall, RecallView::List),
            "Keepsakes\n1  Deploy on Fridays\nChat digests\nkraddie/owl/digest/2  Release  2 facts\nReads the memory of: lint\nAutomatic chat digests are off"
        );
        let shown = render_recall(&recall, RecallView::Show);
        assert!(shown
            .contains("kraddie/owl/digest/2  Release\n    - Tag before pushing\n    - CI is slow"));
    }
}
