import type { AgentPreset } from "../protocol/AgentPreset";
import type { AgentRef } from "../protocol/AgentRef";
import type { ChatCommand } from "../protocol/ChatCommand";
import type { ChatOpen } from "../protocol/ChatOpen";
import type { Command } from "../protocol/Command";
import type { EnvironmentName } from "../protocol/EnvironmentName";
import type { GroupId } from "../protocol/GroupId";
import type { NodeCommand } from "../protocol/NodeCommand";
import type { Session } from "../protocol/Session";
import type { SessionCommand } from "../protocol/SessionCommand";
import type { TerminalId } from "../protocol/TerminalId";
import type { TerminalNode } from "../protocol/TerminalNode";

export type SessionEditCommand = { area: "session" } & SessionCommand;
export type NodeEditCommand = { area: "node" } & NodeCommand;
export type ChatEditCommand = { area: "chat" } & ChatCommand;

export const NEW_SESSION_PREFIX = "Session";
export const NEW_CHAT_LABEL = "New chat";
const TITLE_SEPARATOR = " · ";
const DEFAULT_TEXT_SCALE = 1;

/** The runtime names a session after its group's ordinal; this is the id it will take. */
export function nextSessionId(sessions: readonly Pick<Session, "id">[] | undefined): number {
  return (sessions ?? []).reduce((last, session) => Math.max(last, session.id), 0) + 1;
}

/** A session's title as the runtime builds it: its label, then its number. */
export function nodeTitle(label: string, id: TerminalId): string {
  return `${label}${TITLE_SEPARATOR}${id}`;
}

/**
 * Opens a session with the runtime's default name. The runtime places a new
 * tab in its active session, so the web selects it before opening tabs.
 */
export function newSessionCommand(
  environment: EnvironmentName,
  session: number,
  group: GroupId | null,
): SessionEditCommand {
  return { area: "session", environment, edit: { verb: "open", session, group, name: "", shown: true } };
}

export function selectSessionCommand(environment: EnvironmentName, session: number): SessionEditCommand {
  return { area: "session", environment, edit: { verb: "select", session } };
}

export function renameSessionCommand(
  environment: EnvironmentName,
  session: number,
  name: string,
): SessionEditCommand | undefined {
  const trimmed = name.trim();
  return trimmed
    ? { area: "session", environment, edit: { verb: "rename", session, name: trimmed } }
    : undefined;
}

export function toggleAttentionCommand(environment: EnvironmentName, session: number): SessionEditCommand {
  return { area: "session", environment, edit: { verb: "toggle_attention", session } };
}

export function renameNodeCommand(
  environment: EnvironmentName,
  terminal: TerminalId,
  title: string,
): NodeEditCommand | undefined {
  const trimmed = title.trim();
  return trimmed
    ? { area: "node", environment, edit: { verb: "set", terminal, field: { field: "title", value: trimmed } } }
    : undefined;
}

function closeTerminalCommand(environment: EnvironmentName, terminal: TerminalId): Command {
  return { area: "terminal", verb: "close", terminal: { environment, terminal } };
}

function closeNodeCommand(environment: EnvironmentName, terminal: TerminalId): NodeEditCommand {
  return { area: "node", environment, edit: { verb: "close", terminal } };
}

/** Closing a tab stops its terminal and then drops its node, as the desktop does. */
export function closeNodeCommands(environment: EnvironmentName, terminal: TerminalId): Command[] {
  return [closeTerminalCommand(environment, terminal), closeNodeCommand(environment, terminal)];
}

/** Closing a session closes every terminal and node it holds, then the session. */
export function closeSessionCommands(
  environment: EnvironmentName,
  session: number,
  terminals: readonly TerminalId[],
): Command[] {
  return [
    ...terminals.flatMap((terminal) => closeNodeCommands(environment, terminal)),
    { area: "session", environment, edit: { verb: "close", session } },
  ];
}

export function chatNode(
  id: TerminalId,
  group: GroupId | null,
  harness: AgentPreset,
  title: string,
): TerminalNode {
  return {
    id,
    title,
    state: "Idle",
    harness,
    group,
    text_scale: DEFAULT_TEXT_SCALE,
    kind: "Chat",
  };
}

export function terminalNode(id: TerminalId, group: GroupId | null, title: string): TerminalNode {
  return { id, title, state: "Idle", group, text_scale: DEFAULT_TEXT_SCALE, kind: "Terminal" };
}

export function openNodeCommand(environment: EnvironmentName, node: TerminalNode): NodeEditCommand {
  return { area: "node", environment, edit: { verb: "open", node, place: { at: "tab", anchor: null } } };
}

export function openTerminalCommand(
  environment: EnvironmentName,
  terminal: TerminalId,
  preset: AgentPreset,
  cwd: string | null,
): Command {
  return {
    area: "terminal",
    verb: "open",
    terminal: { environment, terminal },
    intent: { intent: "preset", preset },
    cwd,
  };
}

/** Opens a chat tab's chat: resume the node's session when it has one, else start a new one. */
export function openChatCommand(
  chat: AgentRef,
  node: TerminalNode | undefined,
  newResumeId: () => string,
  harness?: AgentPreset,
): ChatEditCommand {
  const chosen = harness ?? (node?.harness && node.harness !== "Shell" ? node.harness : "ClaudeCode");
  const session: ChatOpen["session"] = node?.resume
    ? { kind: "resume", from: node.resume }
    : { kind: "start", from: newResumeId() };
  return {
    area: "chat",
    verb: "open",
    chat,
    start: {
      harness: chosen,
      model: null,
      effort: null,
      permission_mode: null,
      mcp_servers: null,
      acp: { mode: null, options: [] },
      session,
      messages: [],
      notices: [],
    },
  };
}
