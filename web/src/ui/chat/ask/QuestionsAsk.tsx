import { useState } from "react";
import type { Question } from "../../../protocol/Question";
import { answerQuestionCommand } from "../../../state/commands/chat";
import { answersComplete, pickedAnswers, togglePick, type Picks } from "./questions";
import type { AskProps } from "./types";

export function QuestionsAsk({ chat, ask, send, questions }: AskProps & { questions: Question[] }) {
  const [picks, setPicks] = useState<Picks>({});
  const answers = pickedAnswers(questions, picks);
  return (
    <div className="ask-card">
      {questions.map((question) => (
        <div key={question.question} className="ask-question">
          <span className="title">{question.question}</span>
          <div className="choices">
            {question.options.map((option) => (
              <button
                key={option.label}
                title={option.description ?? undefined}
                className={(picks[question.question] ?? []).includes(option.label) ? "button picked" : "button"}
                onClick={() => setPicks(togglePick(picks, question, option.label))}
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
          disabled={!answersComplete(answers)}
          onClick={() => send(answerQuestionCommand(chat, ask, answers))}
        >
          Answer
        </button>
      </div>
    </div>
  );
}
