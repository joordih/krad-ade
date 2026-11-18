import { useState } from "react";
import type { PermissionMode } from "../../../protocol/PermissionMode";
import { answerAskCommand, approvePlanCommand } from "../../../state/commands/chat";
import { NoteField } from "./NoteField";
import type { AskProps } from "./types";

const FEEDBACK_PLACEHOLDER = "What should the plan change?";

const APPROVALS: { mode: PermissionMode; label: string; primary: boolean }[] = [
  { mode: "acceptEdits", label: "Yes, and accept edits", primary: true },
  { mode: "manual", label: "Yes, approve edits by hand", primary: false },
  { mode: "auto", label: "Yes, and use auto mode", primary: false },
];

export function PlanAsk({ chat, ask, send, wide, plan }: AskProps & { plan: string }) {
  const [planning, setPlanning] = useState(false);
  return (
    <div className="ask-card">
      <span className="title">Approve this plan?</span>
      <pre>{plan}</pre>
      <div className="choices">
        {APPROVALS.map((approval) => (
          <button
            key={approval.mode}
            className={approval.primary ? "button primary" : "button"}
            onClick={() => send(approvePlanCommand(chat, ask, approval.mode))}
          >
            {approval.label}
          </button>
        ))}
        <button className="button" onClick={() => setPlanning(true)}>
          No, keep planning…
        </button>
      </div>
      {planning && (
        <NoteField
          placeholder={FEEDBACK_PLACEHOLDER}
          wide={wide}
          onCancel={() => setPlanning(false)}
          onSend={(message) => {
            setPlanning(false);
            send(answerAskCommand(chat, ask, { deny: { message } }));
          }}
        />
      )}
    </div>
  );
}
