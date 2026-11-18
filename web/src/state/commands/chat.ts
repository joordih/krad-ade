import type { AgentPreset } from "../../protocol/AgentPreset";
import type { AgentRef } from "../../protocol/AgentRef";
import type { AskAnswer } from "../../protocol/AskAnswer";
import type { ChatCommand } from "../../protocol/ChatCommand";
import type { Command } from "../../protocol/Command";
import type { Effort } from "../../protocol/Effort";
import type { Mention } from "../../protocol/Mention";
import type { PermissionMode } from "../../protocol/PermissionMode";
import type { PickedAnswer } from "../../protocol/PickedAnswer";
import { openChatCommand, setNodeHarnessCommand } from "./sessions";

export type ChatAreaCommand = { area: "chat" } & ChatCommand;

const MENTION_LIMIT = 200;

export function promptCommand(chat: AgentRef, text: string, mentions: Mention[], working: boolean): ChatAreaCommand | undefined {
  const trimmed = text.trim();
  if (!trimmed && mentions.length === 0) return undefined;
  const prompt = { text: trimmed, mentions };
  return working ? { area: "chat", verb: "queue", chat, prompt } : { area: "chat", verb: "send", chat, prompt };
}

export function unqueueCommand(chat: AgentRef, index: number): ChatAreaCommand {
  return { area: "chat", verb: "unqueue", chat, index };
}

export function interruptCommand(chat: AgentRef): ChatAreaCommand {
  return { area: "chat", verb: "interrupt", chat };
}

export function setModelCommand(chat: AgentRef, model: string): ChatAreaCommand {
  return { area: "chat", verb: "set_model", chat, model };
}

export function closeChatCommand(chat: AgentRef): ChatAreaCommand {
  return { area: "chat", verb: "close", chat };
}

export function pickModelCommands(
  chat: AgentRef,
  current: AgentPreset,
  harness: AgentPreset,
  model: string,
  newResumeId: () => string,
): Command[] {
  if (harness === current) return [setModelCommand(chat, model)];
  return [
    closeChatCommand(chat),
    setNodeHarnessCommand(chat.environment, chat.terminal, harness),
    openChatCommand(chat, undefined, newResumeId, harness),
    setModelCommand(chat, model),
  ];
}

export function setEffortCommand(chat: AgentRef, effort: Effort): ChatAreaCommand {
  return { area: "chat", verb: "set_effort", chat, effort };
}

export function setPermissionModeCommand(chat: AgentRef, mode: PermissionMode): ChatAreaCommand {
  return { area: "chat", verb: "set_permission_mode", chat, mode };
}

export function pageCommand(chat: AgentRef, before: number, count: number): ChatAreaCommand {
  return { area: "chat", verb: "page", chat, before, count };
}

export function answerAskCommand(chat: AgentRef, ask: number, answer: AskAnswer): ChatAreaCommand {
  return { area: "chat", verb: "answer_ask", chat, ask, answer };
}

export function answerQuestionCommand(chat: AgentRef, ask: number, answers: PickedAnswer[]): ChatAreaCommand {
  return { area: "chat", verb: "answer_question", chat, ask, answers };
}

export function approvePlanCommand(chat: AgentRef, ask: number, mode: PermissionMode): ChatAreaCommand {
  return { area: "chat", verb: "approve_plan", chat, ask, mode };
}

export function mentionsCommand(environment: string, root: string, query: string): ChatAreaCommand {
  return { area: "chat", verb: "mentions", environment, root, query, limit: MENTION_LIMIT };
}
