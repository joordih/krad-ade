mod context;
mod file_index;
mod live;
mod panel;
mod suggest;

pub(super) use file_index::FilesFor;
pub(super) use krad_domain::{fold_entry, AnswerPart, ChatMessage, ShownOutput};
pub(super) use live::StartError;
pub(super) use panel::question_bubble;
use suggest::Suggest;

use gpui_kit::ScrollHandle;
use krad_domain::Skill;

use super::EnvironmentShell;

#[derive(Default)]
pub(super) struct ChatState {
    pub(super) suggest: Option<Suggest>,
    suggest_scroll: ScrollHandle,
    files: file_index::FileSearches,
    skills: Option<Vec<Skill>>,
    pub(super) hosted: super::surface::HostedPlaces,
}
