use gpui_kit::Context;
use krad_domain::agents_api::{ErrorCode, Request, Response};
use krad_domain::GroupId;
use krad_runtime::BusReply;

use super::super::language_servers::CodeCaller;
use super::super::EnvironmentShell;

const NO_CHAT_INBOX: &str = "The chat has no inbox: follow agents with wait and read";
const TERMINALS_ONLY: &str = "Only agents in terminals can do that";

impl EnvironmentShell {
    pub(super) fn answer_chat(
        &mut self,
        environment: &str,
        group: GroupId,
        request: Request,
        reply: BusReply,
        cx: &mut Context<Self>,
    ) {
        let response = match request {
            Request::Code(code) => {
                self.answer_code(environment, &CodeCaller::Group(group), code, reply, cx);
                return;
            }
            Request::List => Response::Agents(self.agent_infos(environment, None)),
            Request::ListAll => Response::Agents(self.all_agent_infos(environment, None)),
            Request::Close(_) => Response::error(ErrorCode::Forbidden, super::ONLY_LEADS_CLOSE),
            Request::Check { .. } => Response::error(ErrorCode::Unsupported, NO_CHAT_INBOX),
            Request::SubscribeAgents(_) => super::feed::streamed_by_the_bus(),
            Request::Hook { .. } | Request::Status(_) | Request::Ask { .. } => {
                Response::error(ErrorCode::Unsupported, TERMINALS_ONLY)
            }
            Request::Askpass { .. } => super::askpass::git_only(),
            Request::Tools => Response::Tools(self.tool_catalog()),
            request @ Request::ExtensionTool { .. } => {
                let caller = CodeCaller::Group(group);
                self.answer_extension_tool(environment, &caller, request, reply, cx);
                return;
            }
            Request::Kraddies
            | Request::RunKraddie { .. }
            | Request::Automations
            | Request::RunAutomation { .. }
            | Request::SwitchAutomation { .. }
            | Request::RememberKraddie { .. }
            | Request::KraddieMemory { .. }
            | Request::KraddieRecall { .. }
            | Request::NewKraddieChat { .. }
            | Request::ShareKraddieMemory { .. }
            | Request::MemorySearch { .. }
            | Request::MemoryRead { .. } => {
                Self::answer_kraddie_request(reply);
                return;
            }
            Request::CreateWorktree { .. }
            | Request::Send { .. }
            | Request::Read { .. }
            | Request::Wait { .. }
            | Request::Spawn { .. }
            | Request::Changes { .. }
            | Request::RunVariants(_)
            | Request::Variants { .. }
            | Request::PickVariant { .. } => super::answered_by_runtime(),
            request @ (Request::Hello
            | Request::Sessions
            | Request::CreateSession { .. }
            | Request::CloseSession { .. }
            | Request::OpenFile { .. }) => {
                self.answer_workspace(environment, Some(group), request, reply, cx);
                return;
            }
        };
        reply.send(response).unwrap_or_default();
    }
}
