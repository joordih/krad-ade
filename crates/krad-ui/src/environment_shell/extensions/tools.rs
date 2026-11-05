use futures::channel::oneshot::Sender as ReplySender;
use gpui_kit::{AppContext as _, Context};
use krad_agents::{
    extension_tool_name, ExtensionToolAnswer, ExtensionToolEffect, ExtensionToolInfo,
    ExtensionToolOutcome, Request, Response, ToolCatalog,
};
use krad_domain::{CodeTools, EnvironmentName, ExtensionTools};
use krad_extensions::api::manifest::{ExtensionManifest, Placement, ToolEffect};
use krad_extensions::api::protocol::{
    CallTool, CallToolParams, CallToolResult, ToolContent, ToolOutcome,
};
use krad_extensions::api::{ExtensionId, ToolName};
use std::collections::HashSet;

use super::super::language_servers::CodeCaller;
use super::super::EnvironmentShell;
use super::{Deferred, SlotKey};

const TOOLS_OFF: &str = "Extension tools are turned off in Settings";
const UNKNOWN_TOOL: &str = "No enabled extension offers the tool";
const NO_ENVIRONMENT: &str = "The agent's environment is not open";
const NOT_A_TOOL_CALL: &str = "Not an extension tool call";
const CONTENT_SEPARATOR: &str = "\n";

pub(in crate::environment_shell) struct ToolCall {
    params: CallToolParams,
    reply: ReplySender<Response>,
}

#[derive(Clone, Debug, PartialEq)]
struct Offered {
    info: ExtensionToolInfo,
    extension: ExtensionId,
    tool: ToolName,
    placement: Placement,
}

fn offered_tools(manifests: &[ExtensionManifest]) -> Vec<Offered> {
    let mut names = HashSet::new();
    let mut offered = Vec::new();
    for manifest in manifests {
        let Some(placement) = manifest.program.as_ref().map(|program| program.placement) else {
            continue;
        };
        for tool in &manifest.contributes.tools {
            let Some(name) = extension_tool_name(manifest.id.as_ref(), tool.name.as_ref()) else {
                tracing::debug!(extension = %manifest.id, tool = %tool.name, "extension tool name too long");
                continue;
            };
            if !names.insert(name.clone()) {
                tracing::debug!(%name, "extension tool name taken by an earlier extension");
                continue;
            }
            offered.push(Offered {
                info: ExtensionToolInfo {
                    name,
                    extension: manifest.name.clone(),
                    description: tool.description.clone(),
                    input_schema: tool.input_schema.clone(),
                    effect: match tool.effect {
                        ToolEffect::ReadOnly => ExtensionToolEffect::ReadOnly,
                        ToolEffect::Destructive => ExtensionToolEffect::Destructive,
                    },
                },
                extension: manifest.id.clone(),
                tool: tool.name.clone(),
                placement,
            });
        }
    }
    offered
}

fn answer_of(result: CallToolResult) -> ExtensionToolAnswer {
    let text = result
        .content
        .into_iter()
        .map(|content| match content {
            ToolContent::Text { text } => text,
        })
        .collect::<Vec<_>>()
        .join(CONTENT_SEPARATOR);
    let outcome = match result.outcome {
        ToolOutcome::Done => ExtensionToolOutcome::Done,
        ToolOutcome::Failed => ExtensionToolOutcome::Failed,
    };
    ExtensionToolAnswer { text, outcome }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn agent_tools_offered(&self) -> bool {
        self.settings.code_tools == CodeTools::On
            || self.settings.extension_tools == ExtensionTools::On
    }

    pub(in crate::environment_shell) fn tool_catalog(&self) -> ToolCatalog {
        let extension_tools = if self.settings.extension_tools == ExtensionTools::On {
            offered_tools(&self.usable_manifests())
                .into_iter()
                .map(|offered| offered.info)
                .collect()
        } else {
            Vec::new()
        };
        ToolCatalog {
            code_tools: self.settings.code_tools,
            extension_tools,
        }
    }

    pub(in crate::environment_shell) fn answer_extension_tool(
        &mut self,
        environment: &str,
        caller: &CodeCaller,
        request: Request,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let refuse = |reply: ReplySender<Response>, message: &str| {
            reply
                .send(Response::Error(message.to_owned()))
                .unwrap_or_default();
        };
        let Request::ExtensionTool { name, arguments } = request else {
            refuse(reply, NOT_A_TOOL_CALL);
            return;
        };
        if self.settings.extension_tools != ExtensionTools::On {
            refuse(reply, TOOLS_OFF);
            return;
        }
        let env = EnvironmentName::new(environment.to_owned());
        let Some(live) = self.envs.get(&env) else {
            refuse(reply, NO_ENVIRONMENT);
            return;
        };
        let group = match caller {
            CodeCaller::Terminal(id) => live.data.group_of(*id),
            CodeCaller::Group(id) => live.data.group(*id),
        };
        let root = group.map(|group| group.root.clone());
        let Some(offered) = offered_tools(&self.usable_manifests())
            .into_iter()
            .find(|offered| offered.info.name == name)
        else {
            refuse(reply, UNKNOWN_TOOL);
            return;
        };
        let key = SlotKey {
            extension: offered.extension,
            environment: match offered.placement {
                Placement::App => None,
                Placement::Environment => Some(env),
            },
        };
        let call = ToolCall {
            params: CallToolParams {
                tool: offered.tool,
                arguments,
                root,
            },
            reply,
        };
        self.ensure_running(key, Deferred::Tool(call), cx);
    }

    pub(in crate::environment_shell) fn run_tool_call(
        &mut self,
        key: &SlotKey,
        call: ToolCall,
        cx: &mut Context<Self>,
    ) {
        let ToolCall { params, reply } = call;
        let Some(connection) = self.extensions.connection(key) else {
            reply
                .send(Response::Error(UNKNOWN_TOOL.to_owned()))
                .unwrap_or_default();
            return;
        };
        cx.background_spawn(async move {
            let response = match connection.request::<CallTool>(&params).await {
                Ok(result) => Response::ExtensionTool(answer_of(result)),
                Err(error) => Response::Error(error.to_string()),
            };
            reply.send(response).unwrap_or_default();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests;
