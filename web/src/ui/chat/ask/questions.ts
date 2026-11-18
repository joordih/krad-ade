import type { PickedAnswer } from "../../../protocol/PickedAnswer";
import type { Question } from "../../../protocol/Question";

export type Picks = Record<string, string[]>;

export function togglePick(picks: Picks, question: Question, label: string): Picks {
  const known = picks[question.question] ?? [];
  if (!question.multiSelect) return { ...picks, [question.question]: [label] };
  const next = known.includes(label) ? known.filter((kept) => kept !== label) : [...known, label];
  return { ...picks, [question.question]: next };
}

export function pickedAnswers(questions: readonly Question[], picks: Picks): PickedAnswer[] {
  return questions.map((question) => ({ question: question.question, labels: picks[question.question] ?? [] }));
}

export function answersComplete(answers: readonly PickedAnswer[]): boolean {
  return answers.every((answer) => answer.labels.length > 0);
}
