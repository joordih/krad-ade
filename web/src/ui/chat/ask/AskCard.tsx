import type { ChatAsk } from "../../../protocol/ChatAsk";
import { answerAskCommand } from "../../../state/commands/chat";
import { PermissionAsk } from "./PermissionAsk";
import { PlanAsk } from "./PlanAsk";
import { QuestionsAsk } from "./QuestionsAsk";
import type { Send } from "./types";

export function AskCard({ ask, send, wide }: { ask: ChatAsk; send: Send; wide: boolean }) {
  const { chat, kind } = ask;
  const props = { chat, ask: ask.ask, send, wide };
  if ("permission" in kind) return <PermissionAsk {...props} permission={kind.permission} canAllowAlways={ask.can_allow_always} />;
  if ("questions" in kind) return <QuestionsAsk {...props} questions={kind.questions} />;
  if ("plan" in kind) return <PlanAsk {...props} plan={kind.plan.plan} />;
  return (
    <div className="ask-card">
      <span className="title">{kind.acp_permission.tool.name}</span>
      <div className="choices">
        {kind.acp_permission.options.map((option) => (
          <button key={option.id} className="button" onClick={() => send(answerAskCommand(chat, ask.ask, { chosen: option.id }))}>
            {option.name}
          </button>
        ))}
      </div>
    </div>
  );
}
