mod context;
mod file_index;
mod live;
mod panel;
mod suggest;

pub(super) use krad_domain::{fold_entry, AnswerPart, ChatMessage, ShownOutput};
pub(super) use live::StartError;
pub(super) use panel::question_bubble;
pub(super) use suggest::workplace_home;
use suggest::Suggest;

use gpui_kit::ScrollHandle;
use krad_chat::Skill;

use super::EnvironmentShell;

#[derive(Default)]
pub(super) struct ChatState {
    pub(super) suggest: Option<Suggest>,
    suggest_scroll: ScrollHandle,
    files: Option<file_index::IndexedFiles>,
    indexing: Option<file_index::IndexKey>,
    skills: Option<Vec<Skill>>,
}
