import { describe, expect, it } from "vitest";
import type { Question } from "../../../protocol/Question";
import { answersComplete, pickedAnswers, togglePick } from "./questions";

function question(text: string, multiSelect: boolean): Question {
  return { question: text, header: null, options: [{ label: "a", description: null }, { label: "b", description: null }], multiSelect };
}

describe("question picks", () => {
  it("keeps one pick for a single choice and toggles picks for a multi choice", () => {
    const single = question("one?", false);
    const multi = question("many?", true);
    let picks = togglePick(togglePick({}, single, "a"), single, "b");
    expect(picks["one?"]).toEqual(["b"]);
    picks = togglePick(togglePick(picks, multi, "a"), multi, "b");
    expect(picks["many?"]).toEqual(["a", "b"]);
    expect(togglePick(picks, multi, "a")["many?"]).toEqual(["b"]);
  });

  it("answers only once every question has a pick", () => {
    const questions = [question("one?", false), question("two?", false)];
    expect(answersComplete(pickedAnswers(questions, { "one?": ["a"] }))).toBe(false);
    expect(answersComplete(pickedAnswers(questions, { "one?": ["a"], "two?": ["b"] }))).toBe(true);
  });
});
