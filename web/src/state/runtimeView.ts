import type { AgentRef } from "../protocol/AgentRef";
import type { ChatMessage } from "../protocol/ChatMessage";
import type { EnvironmentData } from "../protocol/EnvironmentData";
import type { Event } from "../protocol/Event";
import type { Feed } from "../protocol/Feed";
import type { KraddieState } from "../protocol/KraddieState";
import type { Snapshot } from "../protocol/Snapshot";
import {
  attentionCleared,
  attentionFromSnapshot,
  attentionLeft,
  attentionRaised,
  EMPTY_ATTENTION,
  type Attention,
} from "./attention";
import {
  appendToMessage,
  chatsFromSummaries,
  cutMessages,
  setMessage,
  withAsk,
  withChat,
  withEarlier,
  withoutAsk,
  withoutChat,
  type Chats,
  type ChatView,
} from "./chats";
import { applyChange, ordered, withEnvironment } from "./environments";
import { applyFolderChanged, withFileDiagnostics, withListing, type FileDiagnosticMap, type FolderListings } from "./files";
import { withGitRead } from "./git";
import { kraddieKey } from "./kraddies";

export type { ChatKey, ChatView } from "./chats";
export { chatKey } from "./chats";

export type RuntimeView = {
  snapshot: Snapshot | undefined;
  seq: number | undefined;
  environments: EnvironmentData[];
  chats: Chats;
  attention: Attention;
  folders: FolderListings;
  diagnostics: FileDiagnosticMap;
};

export const EMPTY_VIEW: RuntimeView = {
  snapshot: undefined,
  seq: undefined,
  environments: [],
  chats: {},
  attention: EMPTY_ATTENTION,
  folders: {},
  diagnostics: {},
};

export function fromSnapshot(snapshot: Snapshot): RuntimeView {
  return {
    ...EMPTY_VIEW,
    snapshot,
    seq: snapshot.seq,
    environments: snapshot.environments,
    chats: chatsFromSummaries(snapshot.chats),
    attention: attentionFromSnapshot(snapshot.attention),
  };
}

function withKraddies(view: RuntimeView, change: (known: KraddieState[]) => KraddieState[]): RuntimeView {
  if (!view.snapshot) return view;
  const kraddies = view.snapshot.kraddies;
  return { ...view, snapshot: { ...view.snapshot, kraddies: { ...kraddies, kraddies: change(kraddies.kraddies) } } };
}

function changeChat(view: RuntimeView, chat: AgentRef, change: (known: ChatView) => ChatView): RuntimeView {
  return { ...view, chats: withChat(view.chats, chat, change) };
}

export function applyEvent(view: RuntimeView, event: Event): RuntimeView {
  const kind = event.kind;
  switch (kind.type) {
    case "environment_opened":
      return { ...view, environments: withEnvironment(view.environments, kind.environment) };
    case "environment_closed":
      return { ...view, environments: view.environments.filter((known) => known.name !== kind.environment) };
    case "environments_ordered":
      return { ...view, environments: ordered(view.environments, kind.order) };
    case "environment_changed":
      return {
        ...view,
        environments: view.environments.map((known) =>
          known.name === kind.environment ? kind.changes.reduce(applyChange, known) : known,
        ),
      };
    case "settings_changed":
      return view.snapshot ? { ...view, snapshot: { ...view.snapshot, settings: kind.settings } } : view;
    case "attention_raised":
      return { ...view, attention: attentionRaised(view.attention, kind) };
    case "attention_left":
      return { ...view, attention: attentionLeft(view.attention, kind.target) };
    case "attention_history_cleared":
      return { ...view, attention: attentionCleared(view.attention) };
    case "git_read": {
      if (!view.snapshot) return view;
      const { type: _type, ...read } = kind;
      return { ...view, snapshot: { ...view.snapshot, git: withGitRead(view.snapshot.git, read) } };
    }
    case "folder_changed":
      return { ...view, folders: applyFolderChanged(view.folders, kind) };
    case "diagnostics":
      return { ...view, diagnostics: withFileDiagnostics(view.diagnostics, kind) };
    case "kraddie_changed":
      return withKraddies(view, (known) => [
        ...known.filter((state) => kraddieKey(state) !== kraddieKey(kind.kraddie)),
        kind.kraddie,
      ]);
    case "kraddie_removed":
      return withKraddies(view, (known) => known.filter((state) => kraddieKey(state) !== kind.kraddie));
    case "chat_opened":
    case "chat_state":
      return changeChat(view, kind.chat, (chat) => ({ ...chat, state: kind.state }));
    case "chat_message_added":
    case "chat_message_changed":
      return changeChat(view, kind.chat, (chat) => setMessage(chat, kind.index, kind.message));
    case "chat_message_appended":
      return changeChat(view, kind.chat, (chat) => appendToMessage(chat, kind.index, kind.text));
    case "chat_messages_cut":
      return changeChat(view, kind.chat, (chat) => cutMessages(chat, kind.len));
    case "chat_messages_prepended":
      return changeChat(view, kind.chat, (chat) => ({ ...chat, from: chat.from + kind.count }));
    case "chat_ask": {
      const { type: _type, ...ask } = kind;
      return changeChat(view, ask.chat, (chat) => withAsk(chat, ask));
    }
    case "chat_ask_settled":
      return changeChat(view, kind.chat, (chat) => withoutAsk(chat, kind.ask));
    case "chat_closed":
      return { ...view, chats: withoutChat(view.chats, kind.chat) };
    default:
      return view;
  }
}

export function prependPage(view: RuntimeView, chat: AgentRef, from: number, messages: ChatMessage[]): RuntimeView {
  return changeChat(view, chat, (known) => withEarlier(known, from, messages));
}

export function withFolderListing(view: RuntimeView, environment: string, dir: string, entries: RuntimeView["folders"][string]): RuntimeView {
  return { ...view, folders: withListing(view.folders, environment, dir, entries) };
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
