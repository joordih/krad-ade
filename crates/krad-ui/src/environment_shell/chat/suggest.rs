use gpui_kit::component::input::InlineToken;
use gpui_kit::component::input::TextareaState;
use gpui_kit::{AppContext as _, Context, Entity, SharedString, Window};
use krad_chat::{claude_skills, search_skills, shared_skill_message, Skill, SkillSource};
use krad_domain::{parse_wsl_path, wsl_unc_path, AgentPreset, TerminalId, WslPath};
use krad_repo::{index_files, wsl_home, EntryKind, IndexEntry, RepoError};
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

use super::super::terminal_card::home_dir;
use super::super::EnvironmentShell;

pub(super) const MENTION_PREFIX: char = '@';
const SKILL_PREFIX: char = '/';
const FOLDER_SUFFIX: &str = "/";
const WINDOWS_SEPARATOR: char = '\\';
const UNIX_SEPARATOR: &str = "/";
const WORD_GAP: &str = " ";
const SUGGESTION_LIMIT: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum Composer {
    Surface(TerminalId),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::environment_shell) enum SuggestKind {
    Mention,
    Skill,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct Suggest {
    pub(in crate::environment_shell) composer: Composer,
    pub(in crate::environment_shell) kind: SuggestKind,
    range: Range<usize>,
    query: String,
    pub(in crate::environment_shell) selected: usize,
}

pub(in crate::environment_shell) enum Suggestion {
    Entry(IndexEntry),
    Skill(Skill),
}

fn trigger(composer: Composer, text: &str, cursor: usize) -> Option<Suggest> {
    let before = text.get(..cursor)?;
    let start = before
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_whitespace())
        .map_or(0, |(at, ch)| at + ch.len_utf8());
    let word = &before[start..];
    let kind = if word.starts_with(MENTION_PREFIX) {
        SuggestKind::Mention
    } else if word.starts_with(SKILL_PREFIX) {
        SuggestKind::Skill
    } else {
        return None;
    };
    Some(Suggest {
        composer,
        kind,
        range: start..cursor,
        query: word[1..].to_owned(),
        selected: 0,
    })
}

pub(in crate::environment_shell) fn mention_text(entry: &IndexEntry) -> String {
    let path = entry
        .path
        .to_string_lossy()
        .replace(WINDOWS_SEPARATOR, UNIX_SEPARATOR);
    match entry.kind {
        EntryKind::Directory => format!("{path}{FOLDER_SUFFIX}"),
        EntryKind::File => path,
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn update_suggest(
        &mut self,
        composer: Composer,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.composer_input(composer) else {
            return;
        };
        let input = input.read(cx);
        let found = trigger(composer, &input.value(), input.cursor());
        let opened = found.as_ref().map(|found| found.kind).filter(|kind| {
            self.chat
                .suggest
                .as_ref()
                .is_none_or(|open| open.kind != *kind || open.composer != composer)
        });
        let kept = self
            .chat
            .suggest
            .as_ref()
            .is_some_and(|open| open.composer != composer);
        if found.is_some() || !kept {
            self.chat.suggest = found;
        }
        let root = self.composer_root(composer);
        match opened {
            Some(SuggestKind::Mention) => Self::refresh_file_index(root, cx),
            Some(SuggestKind::Skill) => Self::refresh_skills(root, cx),
            None => {}
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn composer_input(
        &self,
        composer: Composer,
    ) -> Option<Entity<TextareaState>> {
        match composer {
            Composer::Surface(id) => self
                .envs
                .get(&self.active)?
                .chats
                .get(&id)
                .map(|chat| chat.input.clone()),
        }
    }

    fn composer_root(&self, composer: Composer) -> Option<PathBuf> {
        match composer {
            Composer::Surface(id) => self
                .envs
                .get(&self.active)?
                .data
                .group_of(id)
                .map(|group| group.root.clone()),
        }
    }

    fn refresh_file_index(root: Option<PathBuf>, cx: &mut Context<Self>) {
        let Some(root) = root else {
            return;
        };
        let indexed = root.clone();
        let index = cx.background_spawn(async move { index_files(&indexed) });
        cx.spawn(async move |this, cx| {
            let index = index.await;
            this.update(cx, |shell, cx| {
                match index {
                    Ok(index) => shell.chat.files = Some((root, index)),
                    Err(error) => shell.toast(&error.to_string(), cx),
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn refresh_skills(root: Option<PathBuf>, cx: &mut Context<Self>) {
        let skills = cx.background_spawn(async move {
            let home = workplace_home(root.as_deref())?;
            Ok::<_, RepoError>(claude_skills(home.as_deref(), root.as_deref()))
        });
        cx.spawn(async move |this, cx| {
            let skills = skills.await;
            this.update(cx, |shell, cx| {
                match skills {
                    Ok(skills) => shell.chat.skills = Some(skills),
                    Err(error) => shell.toast(&error.to_string(), cx),
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn suggestions(&self) -> Option<Vec<Suggestion>> {
        let suggest = self.chat.suggest.as_ref()?;
        match suggest.kind {
            SuggestKind::Mention => {
                let root = self.composer_root(suggest.composer);
                let files = self
                    .chat
                    .files
                    .as_ref()
                    .filter(|(indexed, _)| Some(indexed) == root.as_ref())?
                    .1
                    .search(&suggest.query, SUGGESTION_LIMIT);
                Some(files.into_iter().cloned().map(Suggestion::Entry).collect())
            }
            SuggestKind::Skill => {
                let offered = self.harness_skills(suggest.composer);
                let skills = offered.as_deref().or(self.chat.skills.as_deref())?;
                Some(
                    search_skills(skills, &suggest.query, SUGGESTION_LIMIT)
                        .into_iter()
                        .cloned()
                        .map(Suggestion::Skill)
                        .collect(),
                )
            }
        }
    }

    pub(in crate::environment_shell) fn move_suggestion(
        &mut self,
        down: bool,
        cx: &mut Context<Self>,
    ) {
        let count = self.suggestions().map_or(0, |rows| rows.len());
        let Some(suggest) = self.chat.suggest.as_mut() else {
            return;
        };
        if count == 0 {
            return;
        }
        suggest.selected = if down {
            (suggest.selected + 1) % count
        } else {
            (suggest.selected + count - 1) % count
        };
        self.chat.suggest_scroll.scroll_to_item(suggest.selected);
        cx.notify();
    }

    pub(in crate::environment_shell) fn close_suggest(&mut self, cx: &mut Context<Self>) {
        self.chat.suggest = None;
        cx.notify();
    }

    pub(in crate::environment_shell) fn accept_suggestion(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let picked = self
            .suggestions()
            .and_then(|rows| rows.into_iter().nth(index));
        let Some(suggest) = self.chat.suggest.take() else {
            return;
        };
        let Some(input) = self.composer_input(suggest.composer) else {
            return;
        };
        match picked {
            Some(Suggestion::Entry(entry)) => {
                let text = mention_text(&entry);
                let token = InlineToken::new(text.clone(), format!("{MENTION_PREFIX}{text}"));
                if let Some(mentions) = self.composer_mentions(suggest.composer) {
                    mentions.insert(SharedString::from(text), entry);
                }
                input.update(cx, |input, cx| {
                    if input
                        .replace_range_with_token(suggest.range, token, window, cx)
                        .is_ok()
                    {
                        input.insert(WORD_GAP, window, cx);
                    }
                });
            }
            Some(Suggestion::Skill(skill)) => {
                input.update(cx, |input, cx| {
                    input.set_selected_range(suggest.range, cx);
                    input.replace(
                        format!("{SKILL_PREFIX}{}{WORD_GAP}", skill.name),
                        window,
                        cx,
                    );
                });
            }
            None => {}
        }
        cx.notify();
    }

    fn composer_mentions(
        &mut self,
        composer: Composer,
    ) -> Option<&mut HashMap<SharedString, IndexEntry>> {
        match composer {
            Composer::Surface(id) => self
                .envs
                .get_mut(&self.active)?
                .chats
                .get_mut(&id)
                .map(|chat| &mut chat.mentions),
        }
    }

    fn harness_skills(&self, composer: Composer) -> Option<Vec<Skill>> {
        let Composer::Surface(id) = composer;
        let chat = self.envs.get(&self.active)?.chats.get(&id)?;
        if chat.harness() != AgentPreset::Opencode {
            return None;
        }
        let catalog = chat.structured()?.catalog.as_ref()?;
        Some(
            catalog
                .commands
                .iter()
                .map(|command| Skill {
                    name: command.name.clone(),
                    description: command.description.clone(),
                    source: SkillSource::Claude,
                })
                .collect(),
        )
    }

    pub(in crate::environment_shell) fn shared_skill_message(
        &self,
        message: &str,
    ) -> Option<String> {
        let name = message
            .trim_start()
            .strip_prefix(SKILL_PREFIX)?
            .split_whitespace()
            .next()?;
        let skill = self
            .chat
            .skills
            .as_ref()?
            .iter()
            .find(|skill| skill.name == name)?;
        let SkillSource::Shared(file) = &skill.source else {
            return None;
        };
        Some(shared_skill_message(message, &self.harness_path(file)))
    }
}

pub(in crate::environment_shell) fn workplace_home(
    root: Option<&Path>,
) -> Result<Option<PathBuf>, RepoError> {
    match root.and_then(parse_wsl_path) {
        Some(WslPath { distro, .. }) => Ok(Some(wsl_unc_path(&distro, &wsl_home(&distro)?))),
        None => Ok(home_dir().map(PathBuf::from)),
    }
}

pub(in crate::environment_shell) fn entry_labels(entry: &IndexEntry) -> (String, String) {
    let name = entry
        .path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let folder = entry
        .path
        .parent()
        .map_or_else(String::new, |folder| folder.to_string_lossy().into_owned())
        .replace(WINDOWS_SEPARATOR, UNIX_SEPARATOR);
    (name, folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Option<(SuggestKind, Range<usize>, String)> {
        trigger(Composer::Surface(TerminalId(1)), text, text.len())
            .map(|suggest| (suggest.kind, suggest.range, suggest.query))
    }

    #[test]
    fn an_at_word_asks_for_group_paths_wherever_it_is() {
        assert_eq!(
            found("look at @src/ch"),
            Some((SuggestKind::Mention, 8..15, "src/ch".to_owned()))
        );
        assert_eq!(
            found("@"),
            Some((SuggestKind::Mention, 0..1, String::new()))
        );
        assert_eq!(found("mail me@"), None);
        assert_eq!(found("@src done"), None);
    }

    #[test]
    fn a_claude_code_prompt_takes_a_skill_at_any_word() {
        assert_eq!(
            found("then run /rev"),
            Some((SuggestKind::Skill, 9..13, "rev".to_owned()))
        );
    }

    #[test]
    fn the_word_starts_after_any_whitespace() {
        assert_eq!(
            found("línea\n@a"),
            Some((SuggestKind::Mention, 7..9, "a".to_owned()))
        );
    }

    #[test]
    fn folders_are_written_with_a_trailing_slash() {
        let folder = IndexEntry {
            path: PathBuf::from("src").join("chat"),
            kind: EntryKind::Directory,
        };
        assert_eq!(mention_text(&folder), "src/chat/");
    }
}
