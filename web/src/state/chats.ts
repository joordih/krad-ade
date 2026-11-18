import type { AgentRef } from "../protocol/AgentRef";
import type { ChatAsk } from "../protocol/ChatAsk";
import type { ChatMessage } from "../protocol/ChatMessage";
import type { ChatState } from "../protocol/ChatState";
import type { ChatSummary } from "../protocol/ChatSummary";
import { LOCAL, type RuntimeKey } from "./runtimes";

export type ChatKey = string;

export type ChatView = {
  chat: AgentRef;
  state: ChatState | undefined;
  from: number;
  messages: ChatMessage[];
  asks: ChatAsk[];
};

export type Chats = Record<ChatKey, ChatView>;

export function chatKey(chat: AgentRef): ChatKey {
  return `${chat.environment}/${chat.terminal}`;
}

export function chatsFromSummaries(summaries: readonly ChatSummary[]): Chats {
  return Object.fromEntries(
    summaries.map((summary) => [
      chatKey(summary.tail.chat),
      { chat: summary.tail.chat, state: summary.state, from: summary.tail.from, messages: summary.tail.messages, asks: summary.asks },
    ]),
  );
}

export function harnessSwitchable(runtime: RuntimeKey, chat: Pick<ChatView, "state" | "from" | "messages">): boolean {
  return runtime === LOCAL && chat.from === 0 && chat.messages.length === 0 && !chat.state?.working && !chat.state?.earlier;
}

export function withChat(chats: Chats, chat: AgentRef, change: (known: ChatView) => ChatView): Chats {
  const key = chatKey(chat);
  const known = chats[key] ?? { chat, state: undefined, from: 0, messages: [], asks: [] };
  return { ...chats, [key]: change(known) };
}

export function withoutChat(chats: Chats, chat: AgentRef): Chats {
  const { [chatKey(chat)]: _closed, ...kept } = chats;
  return kept;
}

export function appendText(message: ChatMessage, piece: string): ChatMessage {
  const parts = [...message.parts];
  const last = parts.at(-1);
  if (last && "text" in last) {
    parts[parts.length - 1] = { text: last.text + piece };
  } else {
    parts.push({ text: piece });
  }
  return { ...message, text: message.text + piece, parts };
}

export function setMessage(chat: ChatView, index: number, message: ChatMessage): ChatView {
  const local = index - chat.from;
  if (local < 0 || local > chat.messages.length) return chat;
  const messages = [...chat.messages];
  messages[local] = message;
  return { ...chat, messages };
}

export function appendToMessage(chat: ChatView, index: number, piece: string): ChatView {
  const known = chat.messages[index - chat.from];
  return known ? setMessage(chat, index, appendText(known, piece)) : chat;
}

export function cutMessages(chat: ChatView, len: number): ChatView {
  return { ...chat, messages: chat.messages.slice(0, Math.max(len - chat.from, 0)) };
}

export function withAsk(chat: ChatView, ask: ChatAsk): ChatView {
  return { ...chat, asks: [...chat.asks.filter((known) => known.ask !== ask.ask), ask] };
}

export function withoutAsk(chat: ChatView, ask: number): ChatView {
  return { ...chat, asks: chat.asks.filter((known) => known.ask !== ask) };
}

export function withEarlier(chat: ChatView, from: number, messages: ChatMessage[]): ChatView {
  if (from + messages.length !== chat.from) return chat;
  return { ...chat, from, messages: [...messages, ...chat.messages] };
}
