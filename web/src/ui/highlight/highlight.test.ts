import { describe, expect, it } from "vitest";
import { highlightCode } from "./highlight";
import { buildSyntaxTheme, type SyntaxPalette } from "./theme";

const colors: SyntaxPalette = {
  comment: "#7a7a8c",
  keyword: "#ff79c6",
  string: "#f1fa8c",
  escape: "#ffb86c",
  number: "#bd93f9",
  constant: "#8be9fd",
  attribute: "#50fa7b",
  type: "#8be9fd",
  operator: "#ff79c6",
  function: "#50fa7b",
  property: "#f8f8f2",
  tag: "#ff5555",
  variable: "#f8f8f2",
  punctuation: "#bfbfbf",
};

const theme = buildSyntaxTheme(colors, "#f8f8f2", "#21222c", true, "krad-test");

describe("highlighting a file", () => {
  it("loads the grammar lazily and applies the built theme", async () => {
    const tokens = await highlightCode("// note\nfn main() {}\n", "rust", theme);
    expect(tokens).toBeDefined();
    const all = tokens?.flat() ?? [];
    expect(all.some((token) => token.content.includes("note") && ((token.fontStyle ?? 0) & 1) !== 0)).toBe(true);
    expect(
      all.some(
        (token) =>
          token.content === "main" &&
          token.color?.toLowerCase() === colors.function.toLowerCase(),
      ),
    ).toBe(true);
  });

  it("shows plain lines for a language it cannot load", async () => {
    expect(await highlightCode("hello", "not-a-language", theme)).toBeUndefined();
  });
});
