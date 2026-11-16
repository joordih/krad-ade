import { useState, type KeyboardEvent } from "react";
import type { AgentRef } from "../../protocol/AgentRef";
import type { ChatAsk } from "../../protocol/ChatAsk";
import type { Command } from "../../protocol/Command";
import type { PickedAnswer } from "../../protocol/PickedAnswer";
import type { Question } from "../../protocol/Question";

type Send = (command: Command) => void;

const REASON_PLACEHOLDER = "Tell the agent what to do instead";
const FEEDBACK_PLACEHOLDER = "What should the plan change?";

function answerWith(answer: Parameters<typeof answerOf>[0], chat: AgentRef, ask: number): Command {
  return answerOf(answer, chat, ask);
}

/** The desktop's small note field behind "Deny…" and "No, keep planning…". */
function NoteField({
  placeholder,
  wide,
  onSend,
  onCancel,
}: {
  placeholder: string;
  wide: boolean;
  onSend: (message: string) => void;
  onCancel: () => void;
}) {
  const [text, setText] = useState("");
  const keyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (wide && event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      onSend(text.trim());
    }
  };
  return (
    <div className="note">
      <textarea value={text} onChange={(event) => setText(event.target.value)} onKeyDown={keyDown} placeholder={placeholder} rows={2} />
      <div className="note-bar">
        <button className="button" onClick={onCancel}>
          Cancel
        </button>
        <button className="button primary" onClick={() => onSend(text.trim())}>
          Send
        </button>
      </div>
    </div>
  );
}

function Questions({ chat, ask, questions, send }: { chat: AgentRef; ask: number; questions: Question[]; send: Send }) {
  const [picked, setPicked] = useState<Record<string, string[]>>({});
  const toggle = (question: Question, label: string) => {
    const known = picked[question.question] ?? [];
    const next = question.multiSelect
      ? known.includes(label)
        ? known.filter((kept) => kept !== label)
        : [...known, label]
      : [label];
    setPicked({ ...picked, [question.question]: next });
  };
  const answers: PickedAnswer[] = questions.map((question) => ({
    question: question.question,
    labels: picked[question.question] ?? [],
  }));
  const complete = answers.every((answer) => answer.labels.length > 0);
  return (
    <>
      {questions.map((question) => (
        <div key={question.question} style={{ display: "flex", flexDirection: "column", gap: "var(--krad-space-1-5)" }}>
          <span className="title">{question.question}</span>
          <div className="choices">
            {question.options.map((option) => (
              <button
                key={option.label}
                title={option.description ?? undefined}
                className={(picked[question.question] ?? []).includes(option.label) ? "button picked" : "button"}
                onClick={() => toggle(question, option.label)}
              >
                {option.label}
              </button>
            ))}
          </div>
        </div>
      ))}
      <div className="choices">
        <button
          className="button primary"
          disabled={!complete}
          onClick={() => send({ area: "chat", verb: "answer_question", chat, ask, answers })}
        >
          Answer
        </button>
      </div>
    </>
  );
}

function PermissionAsk({
  chat,
  ask,
  permission,
  canAllowAlways,
  send,
  wide,
}: {
  chat: AgentRef;
  ask: number;
  permission: { tool: string; summary: string; title: string | null };
  canAllowAlways: boolean;
  send: Send;
  wide: boolean;
}) {
  const [denying, setDenying] = useState(false);
  const answer = (value: Parameters<typeof answerOf>[0]) => send(answerWith(value, chat, ask));
  return (
    <div className="ask-card">
      <span className="title">{permission.title ?? `Allow ${permission.tool}?`}</span>
      <pre>{permission.summary}</pre>
      <div className="choices">
        <button className="button primary" onClick={() => answer("allow")}>
          Allow once
        </button>
        {canAllowAlways && (
          <button className="button" onClick={() => answer("allow_always")}>
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
            answer({ deny: { message } });
          }}
        />
      )}
    </div>
  );
}

function PlanAsk({ chat, ask, plan, send, wide }: { chat: AgentRef; ask: number; plan: string; send: Send; wide: boolean }) {
  const [planning, setPlanning] = useState(false);
  return (
    <div className="ask-card">
      <span className="title">Approve this plan?</span>
      <pre>{plan}</pre>
      <div className="choices">
        <button
          className="button primary"
          onClick={() => send({ area: "chat", verb: "approve_plan", chat, ask, mode: "acceptEdits" })}
        >
          Yes, and accept edits
        </button>
        <button className="button" onClick={() => send({ area: "chat", verb: "approve_plan", chat, ask, mode: "manual" })}>
          Yes, approve edits by hand
        </button>
        <button className="button" onClick={() => send({ area: "chat", verb: "approve_plan", chat, ask, mode: "auto" })}>
          Yes, and use auto mode
        </button>
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
            send(answerWith({ deny: { message } }, chat, ask));
          }}
        />
      )}
    </div>
  );
}

export function AskCard({ ask, send, wide = false }: { ask: ChatAsk; send: Send; wide?: boolean }) {
  const { chat, kind } = ask;
  if ("permission" in kind) {
    const permission = kind.permission;
    return (
      <PermissionAsk
        chat={chat}
        ask={ask.ask}
        permission={permission}
        canAllowAlways={ask.can_allow_always}
        send={send}
        wide={wide}
      />
    );
  }
  if ("questions" in kind) {
    return (
      <div className="ask-card">
        <Questions chat={chat} ask={ask.ask} questions={kind.questions} send={send} />
      </div>
    );
  }
  if ("plan" in kind) {
    return <PlanAsk chat={chat} ask={ask.ask} plan={kind.plan.plan} send={send} wide={wide} />;
  }
  return (
    <div className="ask-card">
      <span className="title">{kind.acp_permission.tool.name}</span>
      <div className="choices">
        {kind.acp_permission.options.map((option) => (
          <button
            key={option.id}
            className="button"
            onClick={() => send(answerWith({ chosen: option.id }, chat, ask.ask))}
          >
            {option.name}
          </button>
        ))}
      </div>
    </div>
  );
}

function answerOf(
  answer: "allow" | "allow_always" | { deny: { message: string } } | { chosen: string },
  chat: AgentRef,
  ask: number,
): Command {
  return { area: "chat", verb: "answer_ask", chat, ask, answer };
}
