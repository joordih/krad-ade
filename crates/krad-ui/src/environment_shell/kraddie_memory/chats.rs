use gpui_kit::Context;
use krad_domain::{EnvironmentName, EnvironmentView, KraddieId, Session, SessionEdit, SessionId};
use krad_protocol::KraddieCommand;

use super::super::EnvironmentShell;

impl EnvironmentShell {
    pub(super) fn start_kraddie_chat(&mut self, id: &KraddieId, cx: &mut Context<Self>) {
        let command = KraddieCommand::OpenChat {
            kraddie: id.clone(),
            environment: self.active.clone(),
        };
        self.tell_kraddies(command, cx);
        self.set_view(EnvironmentView::Sessions, cx);
        cx.notify();
    }

    pub(super) fn kraddie_chats<'a>(
        &'a self,
        id: &'a KraddieId,
    ) -> Vec<(EnvironmentName, &'a Session)> {
        self.envs
            .iter()
            .flat_map(|(env, live)| {
                live.data
                    .kraddie_sessions(id)
                    .map(move |session| (env.clone(), session))
            })
            .collect()
    }

    pub(super) fn reopen_kraddie_chat(
        &mut self,
        env: &EnvironmentName,
        session: SessionId,
        cx: &mut Context<Self>,
    ) {
        if *env != self.active {
            return;
        }
        let shown = self
            .edit_environment(env, SessionEdit::Select { session }, cx)
            .applied();
        if shown {
            if let Some(live) = self.envs.get_mut(env) {
                live.view.view = EnvironmentView::Sessions;
            }
            self.save_view();
            cx.notify();
        }
    }
}
