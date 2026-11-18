import { describe, expect, it } from "vitest";
import {
  buildSyntaxTheme,
  isDarkColor,
  isHexColor,
  luminance,
  SYNTAX_NAMES,
  syntaxPalette,
  syntaxRules,
  syntaxThemeFrom,
  type SyntaxPalette,
} from "./theme";

const COLORS: SyntaxPalette = {
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

const PROPS: Record<string, string> = {
  "--krad-text-strong": "#f8f8f2",
  "--krad-bg-panel": "#21222c",
  ...Object.fromEntries(SYNTAX_NAMES.map((name) => [`--krad-syntax-${name}`, COLORS[name]])),
};

function read(property: string): string {
  return PROPS[property] ?? "";
}

describe("the syntax palette", () => {
  it("reads all fourteen properties", () => {
    expect(syntaxPalette(read)).toEqual(COLORS);
    expect(syntaxPalette((property) => (property === "--krad-syntax-tag" ? "" : read(property)))).toBeUndefined();
    expect(syntaxPalette((property) => (property === "--krad-syntax-tag" ? "red" : read(property)))).toBeUndefined();
  });

  it("recognizes the colour forms the sheet writes", () => {
    expect(isHexColor("#12ab34")).toBe(true);
    expect(isHexColor("#12AB34FF")).toBe(true);
    expect(isHexColor("rgb(1 2 3)")).toBe(false);
    expect(isHexColor("#12345")).toBe(false);
  });

  it("tells a dark sheet from a light one by its background", () => {
    expect(isDarkColor("#21222c")).toBe(true);
    expect(isDarkColor("#ffffff")).toBe(false);
    expect(luminance("#000000")).toBe(0);
    expect(luminance("#ffffff")).toBeCloseTo(1);
  });
});

describe("the theme the viewer uses", () => {
  it("maps comments italic, titles bold and embedded code to the text colour", () => {
    const rules = syntaxRules(COLORS, "#f8f8f2");
    const of = (scope: string) => rules.find((rule) => rule.scopes.includes(scope));
    expect(of("comment")).toMatchObject({ foreground: COLORS.comment, fontStyle: "italic" });
    expect(of("punctuation.definition.comment")).toMatchObject({ fontStyle: "italic" });
    expect(of("markup.heading")).toMatchObject({ foreground: COLORS.function, fontStyle: "bold" });
    expect(of("meta.embedded")).toMatchObject({ foreground: "#f8f8f2" });
    expect(of("constant.language")).toMatchObject({ foreground: COLORS.constant });
    expect(of("constant.numeric")).toMatchObject({ foreground: COLORS.number });
    expect(of("keyword.operator")).toMatchObject({ foreground: COLORS.operator });
    expect(of("entity.name.tag")).toMatchObject({ foreground: COLORS.tag });
    expect(of("variable.other.property")).toMatchObject({ foreground: COLORS.property });
    expect(of("punctuation")).toMatchObject({ foreground: COLORS.punctuation });
  });

  it("builds a Shiki theme with the sheet's colours and appearance", () => {
    const theme = buildSyntaxTheme(COLORS, "#f8f8f2", "#21222c", true, "test-theme");
    expect(theme).toMatchObject({
      name: "test-theme",
      type: "dark",
      colors: { "editor.background": "#21222c", "editor.foreground": "#f8f8f2" },
    });
    expect(theme.settings[0]?.settings).toMatchObject({ foreground: "#f8f8f2", background: "#21222c" });
    expect(theme.settings).toHaveLength(1 + syntaxRules(COLORS, "#f8f8f2").length);
    expect(buildSyntaxTheme(COLORS, "#111111", "#ffffff", false).type).toBe("light");
  });

  it("makes a new theme when the sheet reloads, and none before it loads", () => {
    const first = syntaxThemeFrom(read);
    const second = syntaxThemeFrom(read);
    expect(first?.name).toMatch(/^krad-editor-/);
    expect(second?.name).not.toBe(first?.name);
    expect(first?.type).toBe("dark");
    expect(syntaxThemeFrom((property) => (property === "--krad-text-strong" ? "" : read(property)))).toBeUndefined();
  });
});
