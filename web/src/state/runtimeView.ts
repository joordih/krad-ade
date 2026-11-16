import type { AgentRef } from "../protocol/AgentRef";
import type { ChatAsk } from "../protocol/ChatAsk";
import type { ChatMessage } from "../protocol/ChatMessage";
import type { ChatState } from "../protocol/ChatState";
import type { EnvironmentChange } from "../protocol/EnvironmentChange";
import type { EnvironmentData } from "../protocol/EnvironmentData";
import type { Event } from "../protocol/Event";
import type { Feed } from "../protocol/Feed";
import type { Snapshot } from "../protocol/Snapshot";

export type ChatKey = string;

export type ChatView = {
  chat: AgentRef;
  state: ChatState | undefined;
  from: number;
  messages: ChatMessage[];
  asks: ChatAsk[];
};

export type RuntimeView = {
  snapshot: Snapshot | undefined;
  seq: number | undefined;
  environments: EnvironmentData[];
  chats: Record<ChatKey, ChatView>;
};

export const EMPTY_VIEW: RuntimeView = { snapshot: undefined, seq: undefined, environments: [], chats: {} };

export function chatKey(chat: AgentRef): ChatKey {
  return `${chat.environment}/${chat.terminal}`;
}

function upsert<T>(items: T[], index: number, item: T, same: (known: T) => boolean): T[] {
  const kept = items.filter((known) => !same(known));
  const at = Math.min(Math.max(index, 0), kept.length);
  return [...kept.slice(0, at), item, ...kept.slice(at)];
}

export function applyChange(environment: EnvironmentData, change: EnvironmentChange): EnvironmentData {
  switch (change.change) {
    case "node":
      return { ...environment, nodes: upsert(environment.nodes, change.index, change.node, (node) => node.id === change.node.id) };
    case "node_removed":
      return { ...environment, nodes: environment.nodes.filter((node) => node.id !== change.id) };
    case "group":
      return { ...environment, groups: upsert(environment.groups, change.index, change.group, (group) => group.id === change.group.id) };
    case "group_removed":
      return { ...environment, groups: environment.groups.filter((group) => group.id !== change.id) };
    case "session":
      return {
        ...environment,
        sessions: upsert(environment.sessions ?? [], change.index, change.session, (session) => session.id === change.session.id),
      };
    case "session_removed":
      return { ...environment, sessions: (environment.sessions ?? []).filter((session) => session.id !== change.id) };
    case "active_session":
      return { ...environment, active_session: change.session };
    case "next_id":
      return { ...environment, next_id: change.next };
    case "title":
      return { ...environment, title: change.title };
    case "host":
      return { ...environment, host: change.host };
    default:
      return environment;
  }
}

function appendText(message: ChatMessage, piece: string): ChatMessage {
  const parts = [...message.parts];
  const last = parts.at(-1);
  if (last && "text" in last) {
    parts[parts.length - 1] = { text: last.text + piece };
  } else {
    parts.push({ text: piece });
  }
  return { ...message, text: message.text + piece, parts };
}

function setMessage(chat: ChatView, index: number, message: ChatMessage): ChatView {
  const local = index - chat.from;
  if (local < 0 || local > chat.messages.length) return chat;
  const messages = [...chat.messages];
  messages[local] = message;
  return { ...chat, messages };
}

function withChat(view: RuntimeView, chat: AgentRef, change: (known: ChatView) => ChatView): RuntimeView {
  const key = chatKey(chat);
  const known = view.chats[key] ?? { chat, state: undefined, from: 0, messages: [], asks: [] };
  return { ...view, chats: { ...view.chats, [key]: change(known) } };
}

export function fromSnapshot(snapshot: Snapshot): RuntimeView {
  const chats: Record<ChatKey, ChatView> = {};
  for (const summary of snapshot.chats) {
    chats[chatKey(summary.tail.chat)] = {
      chat: summary.tail.chat,
      state: summary.state,
      from: summary.tail.from,
      messages: summary.tail.messages,
      asks: summary.asks,
    };
  }
  return { snapshot, seq: snapshot.seq, environments: snapshot.environments, chats };
}

export function applyEvent(view: RuntimeView, event: Event): RuntimeView {
  const kind = event.kind;
  switch (kind.type) {
    case "environment_opened":
      return {
        ...view,
        environments: [...view.environments.filter((known) => known.name !== kind.environment.name), kind.environment],
      };
    case "environment_closed":
      return { ...view, environments: view.environments.filter((known) => known.name !== kind.environment) };
    case "environments_ordered":
      return {
        ...view,
        environments: kind.order
          .map((name) => view.environments.find((known) => known.name === name))
          .filter((known): known is EnvironmentData => known !== undefined),
      };
    case "environment_changed":
      return {
        ...view,
        environments: view.environments.map((known) =>
          known.name === kind.environment ? kind.changes.reduce(applyChange, known) : known,
        ),
      };
    case "settings_changed":
      return view.snapshot ? { ...view, snapshot: { ...view.snapshot, settings: kind.settings } } : view;
    case "chat_opened":
    case "chat_state":
      return withChat(view, kind.chat, (chat) => ({ ...chat, state: kind.state }));
    case "chat_message_added":
    case "chat_message_changed":
      return withChat(view, kind.chat, (chat) => setMessage(chat, kind.index, kind.message));
    case "chat_message_appended":
      return withChat(view, kind.chat, (chat) => {
        const known = chat.messages[kind.index - chat.from];
        return known ? setMessage(chat, kind.index, appendText(known, kind.text)) : chat;
      });
    case "chat_messages_cut":
      return withChat(view, kind.chat, (chat) => ({
        ...chat,
        messages: chat.messages.slice(0, Math.max(kind.len - chat.from, 0)),
      }));
    case "chat_messages_prepended":
      return withChat(view, kind.chat, (chat) => ({ ...chat, from: chat.from + kind.count }));
    case "chat_ask": {
      const { type: _type, ...ask } = kind;
      return withChat(view, ask.chat, (chat) => ({ ...chat, asks: [...chat.asks.filter((known) => known.ask !== ask.ask), ask] }));
    }
    case "chat_ask_settled":
      return withChat(view, kind.chat, (chat) => ({ ...chat, asks: chat.asks.filter((known) => known.ask !== kind.ask) }));
    case "chat_closed": {
      const { [chatKey(kind.chat)]: _closed, ...chats } = view.chats;
      return { ...view, chats };
    }
    default:
      return view;
  }
}

export function prependPage(view: RuntimeView, chat: AgentRef, from: number, messages: ChatMessage[]): RuntimeView {
  return withChat(view, chat, (known) => {
    if (from + messages.length !== known.from) return known;
    return { ...known, from, messages: [...messages, ...known.messages] };
  });
}

export function applyFeed(view: RuntimeView, feed: Feed): RuntimeView {
  if (feed.feed === "snapshot") {
    const { feed: _feed, ...snapshot } = feed;
    return fromSnapshot(snapshot);
  }
  const { feed: _feed, ...event } = feed;
  if (view.seq !== undefined && event.seq <= view.seq) return view;
  return { ...applyEvent(view, event), seq: event.seq };
}
