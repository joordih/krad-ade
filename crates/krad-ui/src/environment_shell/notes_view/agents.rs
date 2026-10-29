use gpui_kit::Context;
use krad_domain::{continue_note_prompt, VAULT_DIR};

use super::super::sessions::Launch;
use super::super::EnvironmentShell;

const TERMINAL_LABEL: &str = "notes";
const ASKED_TEXT: &str = "An agent is writing the next section in a new terminal";

impl EnvironmentShell {
    pub(super) fn continue_note_with_agent(&mut self, path: &str, cx: &mut Context<Self>) {
        self.save_note(path, cx);
        let (Some(preset), model) = self.default_agent_choice() else {
            return;
        };
        let note = format!("{VAULT_DIR}/{path}");
        let env = self.active.clone();
        let group = self.active_group().map(|group| group.id);
        self.spawn_terminal_in(
            &env,
            group,
            TERMINAL_LABEL,
            Launch::Agent {
                preset,
                prompt: continue_note_prompt(&note),
                model,
            },
            None,
            cx,
        );
        self.push_toast(ASKED_TEXT.to_owned());
        cx.notify();
    }
}
