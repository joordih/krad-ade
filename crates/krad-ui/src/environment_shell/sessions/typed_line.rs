use krad_chat::{prepare_opencode_session, OpencodeServer};
use krad_domain::{attach_line, EnvironmentName, ModelId, ResumeId, TerminalId};
use krad_protocol::WriteBy;
use krad_runtime::RuntimeHandle;
use std::path::PathBuf;
use std::sync::Arc;

use super::super::live_terminal::terminal_ref;
use super::super::shell_site::ShellSite;

const SHELL_SUBMIT: &str = "\r";

pub(super) struct TypedLine {
    agent: String,
    site: ShellSite,
    cwd: Option<PathBuf>,
    model: Option<ModelId>,
}

impl TypedLine {
    pub(super) const fn new(
        agent: String,
        site: ShellSite,
        cwd: Option<PathBuf>,
        model: Option<ModelId>,
    ) -> Self {
        Self {
            agent,
            site,
            cwd,
            model,
        }
    }
}

impl TypedLine {
    pub(super) fn bytes(self, attached: Option<&(Arc<OpencodeServer>, ResumeId)>) -> Vec<u8> {
        let clear = self.site.clear_prefix();
        let agent = match attached {
            Some((server, resume)) => attach_line(&self.agent, &server.url(), resume),
            None => self.agent,
        };
        format!("{clear}{agent}{SHELL_SUBMIT}").into_bytes()
    }
}

pub(super) fn type_attached_line(
    runtime: RuntimeHandle,
    (env, id): (&EnvironmentName, TerminalId),
    typed: TypedLine,
    attached: (Arc<OpencodeServer>, ResumeId),
) {
    let terminal = terminal_ref(env, id);
    std::thread::spawn(move || {
        let (server, resume) = &attached;
        let directory = typed.cwd.clone().unwrap_or_default();
        prepare_opencode_session(server, resume, &directory, typed.model.as_ref())
            .unwrap_or_default();
        runtime.write_terminal(terminal, typed.bytes(Some(&attached)), WriteBy::App);
    });
}
