import { useState } from "react";
import type { AgentRef } from "../../protocol/AgentRef";
import type { ChatAsk } from "../../protocol/ChatAsk";
import type { Command } from "../../protocol/Command";
import type { PickedAnswer } from "../../protocol/PickedAnswer";
import type { Question } from "../../protocol/Question";

type Send = (command: Command) => void;

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

export function AskCard({ ask, send }: { ask: ChatAsk; send: Send }) {
  const { chat, kind } = ask;
  const answer = (answer: Parameters<typeof answerOf>[0]) => send(answerOf(answer, chat, ask.ask));
  if ("permission" in kind) {
    const permission = kind.permission;
    return (
      <div className="ask-card">
        <span className="title">{permission.title ?? `Allow ${permission.tool}?`}</span>
        <pre>{permission.summary}</pre>
        <div className="choices">
          <button className="button primary" onClick={() => answer("allow")}>
            Allow
          </button>
          {ask.can_allow_always && (
            <button className="button" onClick={() => answer("allow_always")}>
              Always allow
            </button>
          )}
          <button className="button" onClick={() => answer({ deny: { message: "" } })}>
            Deny
          </button>
        </div>
      </div>
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
    return (
      <div className="ask-card">
        <span className="title">Approve the plan?</span>
        <pre>{kind.plan.plan}</pre>
        <div className="choices">
          <button className="button primary" onClick={() => send({ area: "chat", verb: "approve_plan", chat, ask: ask.ask, mode: "acceptEdits" })}>
            Approve and accept edits
          </button>
          <button className="button" onClick={() => send({ area: "chat", verb: "approve_plan", chat, ask: ask.ask, mode: "manual" })}>
            Approve, ask before edits
          </button>
          <button className="button" onClick={() => answer({ deny: { message: "" } })}>
            Keep planning
          </button>
        </div>
      </div>
    );
  }
  return (
    <div className="ask-card">
      <span className="title">{kind.acp_permission.tool.name}</span>
      <div className="choices">
        {kind.acp_permission.options.map((option) => (
          <button key={option.id} className="button" onClick={() => answer({ chosen: option.id })}>
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
