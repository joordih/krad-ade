use krad_domain::{Prompt, RewindScope, TerminalId};

use super::super::super::EnvironmentShell;
use super::super::ChatPlace;
use super::{structured_chat, structured_mut, StructuredChat};

const STARTING_TEXT: &str = "The chat is still starting; send the edit again in a moment";

impl StructuredChat {
    pub(in crate::environment_shell::surface) const fn is_rewinding(&self) -> bool {
        self.rewinding
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell::surface) fn start_rewind(
        &mut self,
        place: &ChatPlace,
        id: TerminalId,
        index: usize,
        prompt: Prompt,
        scope: RewindScope,
    ) {
        let rewinds = structured_chat(self, place, id)
            .and_then(|chat| chat.chat.messages.get(index))
            .is_some_and(|message| message.rewind.is_some());
        if !rewinds {
            return;
        }
        if !self.rewind_in_runtime(place, id, (index, prompt), scope) {
            if let Some(chat) = structured_chat(self, place, id) {
                chat.chat
                    .problem
                    .get_or_insert_with(|| STARTING_TEXT.to_owned());
            }
            return;
        }
        if let Some(structured) = structured_mut(self, place, id) {
            structured.working = true;
            structured.rewinding = true;
        }
    }
}
