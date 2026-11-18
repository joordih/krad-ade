import { describe, expect, it } from "vitest";
import { accepted, entryLabels, matchingSkills, mentionText, suggestTrigger, typedKey } from "./suggestions";

describe("composer suggestions", () => {
  it("asks for files after an @ word and for skills after a / word, anywhere in the prompt", () => {
    expect(suggestTrigger("look at @src/ma", 15)).toEqual({ kind: "mention", start: 8, end: 15, query: "src/ma" });
    expect(suggestTrigger("then /rev", 9)).toEqual({ kind: "skill", start: 5, end: 9, query: "rev" });
    expect(suggestTrigger("plain words", 11)).toBeUndefined();
    expect(suggestTrigger("a@b", 3)).toBeUndefined();
  });

  it("replaces the word with the picked value and a trailing gap", () => {
    const trigger = { kind: "mention" as const, start: 8, end: 11, query: "sr" };
    expect(accepted("look at @sr now", trigger, mentionText("src", true))).toEqual({ text: "look at @src/  now", cursor: 14 });
    expect(accepted("/re", { kind: "skill", start: 0, end: 3, query: "re" }, "review")).toEqual({ text: "/review ", cursor: 8 });
  });

  it("types the key at the cursor, apart from the word before it", () => {
    expect(typedKey("hello", 5, "mention")).toEqual({ text: "hello @", cursor: 7 });
    expect(typedKey("", 0, "skill")).toEqual({ text: "/", cursor: 1 });
  });

  it("splits an entry into its name and folder and filters skills", () => {
    expect(entryLabels("src/ui/App.tsx")).toEqual({ name: "App.tsx", folder: "src/ui" });
    expect(entryLabels("README.md")).toEqual({ name: "README.md", folder: "" });
    const skills = [{ name: "review", description: "Review a PR" }, { name: "init", description: "Write CLAUDE.md" }];
    expect(matchingSkills(skills, "pr").map((skill) => skill.name)).toEqual(["review"]);
  });
});
