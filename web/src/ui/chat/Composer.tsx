import { useState, type KeyboardEvent } from "react";
import type { AgentRef } from "../../protocol/AgentRef";
import type { ChatState } from "../../protocol/ChatState";
import type { Command } from "../../protocol/Command";
import { Icon } from "../Icon";

const PERMISSION_LABEL: Record<string, string> = {
  manual: "Ask before edits",
  acceptEdits: "Accept edits",
  plan: "Plan",
  auto: "Auto",
  dontAsk: "Don't ask",
  bypassPermissions: "Bypass permissions",
};

export function Composer({
  chat,
  state,
  send,
  wide,
}: {
  chat: AgentRef;
  state: ChatState | undefined;
  send: (command: Command) => void;
  wide: boolean;
}) {
  const [text, setText] = useState("");
  const working = state?.working ?? false;

  function submit() {
    const prompt = { text: text.trim(), mentions: [] };
    if (!prompt.text) return;
    send(working ? { area: "chat", verb: "queue", chat, prompt } : { area: "chat", verb: "send", chat, prompt });
    setText("");
  }

  function keyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (wide && event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      submit();
    }
  }

  return (
    <div className="composer">
      <textarea
        value={text}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={keyDown}
        placeholder={working ? "Queue a message…" : "Ask anything…"}
        rows={2}
      />
      <div className="bar">
        <Icon name="square-lock-02" small />
        <span>{PERMISSION_LABEL[state?.permission_mode ?? "manual"] ?? state?.permission_mode}</span>
        <span className="spacer" />
        <span>{state?.model ?? ""}</span>
        {state?.queued.length ? <span>{state.queued.length} queued</span> : null}
        {working && (
          <button className="round" title="Interrupt" onClick={() => send({ area: "chat", verb: "interrupt", chat })}>
            <Icon name="stop-circle" small />
          </button>
        )}
        <button className="round primary" title={working ? "Queue" : "Send"} onClick={submit}>
          <Icon name="sent" small />
        </button>
      </div>
    </div>
  );
}
