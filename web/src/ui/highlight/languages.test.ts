import { describe, expect, it } from "vitest";
import { LANGUAGE_LOADERS, languageFor, languageLabel } from "./languages";

describe("the viewer's language map", () => {
  it("takes a whole file name before a suffix", () => {
    expect(languageFor("Dockerfile")).toBe("docker");
    expect(languageFor("Makefile")).toBe("make");
    expect(languageFor("CMakeLists.txt")).toBe("cmake");
    expect(languageFor("Gemfile")).toBe("ruby");
    expect(languageFor(".zshrc")).toBe("shell");
  });

  it("takes the longest suffix of a dotted name", () => {
    expect(languageFor("auth.test.ts")).toBe("typescript");
    expect(languageFor("main.rs")).toBe("rust");
    expect(languageFor("index.d.ts")).toBe("typescript");
    expect(languageFor("settings.JSON")).toBe("json");
    expect(languageFor("button.module.css")).toBe("css");
  });

  it("calls everything else plain text and labels the rest", () => {
    expect(languageFor("notes.unknown")).toBeUndefined();
    expect(languageFor("LICENSE")).toBeUndefined();
    expect(languageLabel("main.rs")).toBe("Rust");
    expect(languageLabel("LICENSE")).toBe("Plain text");
  });

  it("only names grammars it can lazily load", () => {
    for (const language of [
      "typescript",
      "rust",
      "json",
      "shell",
      "html",
      "css",
      "markdown",
      "docker",
    ]) {
      expect(LANGUAGE_LOADERS[language]).toBeTypeOf("function");
    }
  });
});
