import { useState } from "react";
import { answerAskCommand } from "../../../state/commands/chat";
import { NoteField } from "./NoteField";
import type { AskProps } from "./types";

const REASON_PLACEHOLDER = "Tell the agent what to do instead";

export function PermissionAsk({
  chat,
  ask,
  send,
  wide,
  permission,
  canAllowAlways,
}: AskProps & { permission: { tool: string; summary: string; title: string | null }; canAllowAlways: boolean }) {
  const [denying, setDenying] = useState(false);
  return (
    <div className="ask-card">
      <span className="title">{permission.title ?? `Allow ${permission.tool}?`}</span>
      <pre>{permission.summary}</pre>
      <div className="choices">
        <button className="button primary" onClick={() => send(answerAskCommand(chat, ask, "allow"))}>
          Allow once
        </button>
        {canAllowAlways && (
          <button className="button" onClick={() => send(answerAskCommand(chat, ask, "allow_always"))}>
            Always allow
          </button>
        )}
        <button className="button" onClick={() => setDenying(true)}>
          Deny…
        </button>
      </div>
      {denying && (
        <NoteField
          placeholder={REASON_PLACEHOLDER}
          wide={wide}
          onCancel={() => setDenying(false)}
          onSend={(message) => {
            setDenying(false);
            send(answerAskCommand(chat, ask, { deny: { message } }));
          }}
        />
      )}
    </div>
  );
}
