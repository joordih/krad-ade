import { describe, expect, it } from "vitest";
import type { ChatMessage } from "../protocol/ChatMessage";
import type { Event } from "../protocol/Event";
import { applyEvent, chatKey, EMPTY_VIEW, prependPage, type RuntimeView } from "./runtimeView";
import { diagnosticsKey, folderKey } from "./files";

const chat = { environment: "main", terminal: 4 };

function message(text: string, speaker: ChatMessage["speaker"] = "assistant"): ChatMessage {
  return {
    speaker,
    text,
    parts: text ? [{ text }] : [],
    error: null,
    running: false,
    mentions: [],
    checklist: null,
    rewind: null,
    tool_outputs: {},
  };
}

function event(kind: Event["kind"], seq = 1): Event {
  return { seq, kind };
}

function opened(): RuntimeView {
  return {
    ...EMPTY_VIEW,
    chats: { [chatKey(chat)]: { chat, state: undefined, from: 10, messages: [message("hi", "user")], asks: [] } },
  };
}

describe("chat events", () => {
  it("adds, streams and cuts messages by their absolute index", () => {
    let view = opened();
    view = applyEvent(view, event({ type: "chat_message_added", chat, index: 11, message: message("") }));
    view = applyEvent(view, event({ type: "chat_message_appended", chat, index: 11, text: "Hel" }));
    view = applyEvent(view, event({ type: "chat_message_appended", chat, index: 11, text: "lo" }));
    const shown = view.chats[chatKey(chat)]!;
    expect(shown.messages[1]?.text).toBe("Hello");
    expect(shown.messages[1]?.parts).toEqual([{ text: "Hello" }]);
    view = applyEvent(view, event({ type: "chat_messages_cut", chat, len: 11 }));
    expect(view.chats[chatKey(chat)]!.messages).toHaveLength(1);
  });

  it("starts a new text part after a tool and keeps earlier pages in order", () => {
    let view = opened();
    const tool: ChatMessage = { ...message(""), parts: [{ tool: { name: "Bash", kind: "Shell", summary: "ls" } }] };
    view = applyEvent(view, event({ type: "chat_message_added", chat, index: 11, message: tool }));
    view = applyEvent(view, event({ type: "chat_message_appended", chat, index: 11, text: "done" }));
    expect(view.chats[chatKey(chat)]!.messages[1]?.parts.at(-1)).toEqual({ text: "done" });
    view = prependPage(view, chat, 8, [message("a", "user"), message("b")]);
    const shown = view.chats[chatKey(chat)]!;
    expect(shown.from).toBe(8);
    expect(shown.messages.map((known) => known.text)).toEqual(["a", "b", "hi", "done"]);
  });

  it("holds an ask until it is settled", () => {
    let view = opened();
    const ask = { chat, ask: 3, kind: { plan: { plan: "1. test" } }, can_allow_always: false };
    view = applyEvent(view, event({ type: "chat_ask", ...ask }));
    expect(view.chats[chatKey(chat)]!.asks).toHaveLength(1);
    view = applyEvent(view, event({ type: "chat_ask_settled", chat, ask: 3 }));
    expect(view.chats[chatKey(chat)]!.asks).toHaveLength(0);
  });
});

describe("file events", () => {
  it("keeps watched folder listings and a file's diagnostics", () => {
    let view = applyEvent(
      EMPTY_VIEW,
      event({
        type: "folder_changed",
        environment: "main",
        dir: "/r",
        entries: [{ name: "main.rs", path: "/r/main.rs", kind: "file" }],
      }),
    );
    expect(view.folders[folderKey("main", "/r")]?.map((entry) => entry.name)).toEqual(["main.rs"]);
    view = applyEvent(
      view,
      event(
        {
          type: "diagnostics",
          environment: "main",
          path: "/r/main.rs",
          diagnostics: [
            {
              range: { start: { line: 2, character: 4 }, end: { line: 2, character: 9 } },
              severity: "warning",
              message: "unused",
            },
          ],
        },
        2,
      ),
    );
    expect(view.diagnostics[diagnosticsKey("main", "/r/main.rs")]?.[0]?.message).toBe("unused");
  });
});

describe("attention events", () => {
  const target = { environment: "main", terminal: 4 };
  const notice = { target, kind: "permission" as const, title: "agent 4", at: "2026-11-16T10:01:00Z" };

  it("raises, leaves and clears the queue", () => {
    let view = applyEvent(EMPTY_VIEW, event({ type: "attention_raised", ...notice }));
    expect(view.attention.waiting).toHaveLength(1);
    expect(view.attention.history).toHaveLength(1);
    view = applyEvent(view, event({ type: "attention_left", target }, 2));
    expect(view.attention.waiting).toHaveLength(0);
    view = applyEvent(view, event({ type: "attention_history_cleared" }, 3));
    expect(view.attention.history).toHaveLength(0);
  });
});
