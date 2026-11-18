import type { ThemeRegistrationRaw } from "shiki/core";

export const SYNTAX_NAMES = [
  "comment",
  "keyword",
  "string",
  "escape",
  "number",
  "constant",
  "attribute",
  "type",
  "operator",
  "function",
  "property",
  "tag",
  "variable",
  "punctuation",
] as const;

export type SyntaxName = (typeof SYNTAX_NAMES)[number];
export type SyntaxPalette = Record<SyntaxName, string>;

export const SYNTAX_PROPERTY_PREFIX = "--krad-syntax-";
const FOREGROUND_PROPERTY = "--krad-text-strong";
const BACKGROUND_PROPERTY = "--krad-bg-panel";
const THEME_NAME = "krad-editor";
const DARK_LUMINANCE = 0.5;
const HEX_COLOR = /^#[0-9a-f]{6}([0-9a-f]{2})?$/i;
const HEX_RADIX = 16;
const FULL_BYTE = 255;
const RED_SHIFT = 0;
const GREEN_SHIFT = 2;
const BLUE_SHIFT = 4;
const SRGB_LINEAR_LIMIT = 0.03928;
const SRGB_DIVISOR = 12.92;
const SRGB_OFFSET = 0.055;
const SRGB_SCALE = 1.055;
const SRGB_EXPONENT = 2.4;
const RED_WEIGHT = 0.2126;
const GREEN_WEIGHT = 0.7152;
const BLUE_WEIGHT = 0.0722;

export type SyntaxThemeRule = {
  scopes: string[];
  foreground: string;
  fontStyle?: "italic" | "bold";
};

export function isHexColor(color: string): boolean {
  return HEX_COLOR.test(color);
}

export function syntaxPalette(read: (property: string) => string): SyntaxPalette | undefined {
  const palette = {} as SyntaxPalette;
  for (const name of SYNTAX_NAMES) {
    const color = read(`${SYNTAX_PROPERTY_PREFIX}${name}`).trim();
    if (!isHexColor(color)) return undefined;
    palette[name] = color;
  }
  return palette;
}

function channel(color: string, offset: number): number {
  const body = color.slice(1);
  return parseInt(body.slice(offset, offset + 2), HEX_RADIX) / FULL_BYTE;
}

function linearize(value: number): number {
  return value <= SRGB_LINEAR_LIMIT
    ? value / SRGB_DIVISOR
    : ((value + SRGB_OFFSET) / SRGB_SCALE) ** SRGB_EXPONENT;
}

export function luminance(color: string): number {
  return (
    RED_WEIGHT * linearize(channel(color, RED_SHIFT)) +
    GREEN_WEIGHT * linearize(channel(color, GREEN_SHIFT)) +
    BLUE_WEIGHT * linearize(channel(color, BLUE_SHIFT))
  );
}

export function isDarkColor(color: string): boolean {
  return luminance(color) < DARK_LUMINANCE;
}

export function syntaxRules(palette: SyntaxPalette, foreground: string): SyntaxThemeRule[] {
  return [
    {
      scopes: ["comment", "punctuation.definition.comment"],
      foreground: palette.comment,
      fontStyle: "italic",
    },
    {
      scopes: [
        "keyword",
        "keyword.control",
        "keyword.other",
        "keyword.declaration",
        "storage.modifier",
        "meta.preprocessor",
        "entity.name.label",
      ],
      foreground: palette.keyword,
    },
    {
      scopes: ["string", "string.quoted", "string.template", "string.unquoted", "markup.inline.raw"],
      foreground: palette.string,
    },
    {
      scopes: [
        "constant.character.escape",
        "string.regexp",
        "string.special",
        "constant.other.placeholder",
      ],
      foreground: palette.escape,
    },
    {
      scopes: ["constant.numeric"],
      foreground: palette.number,
    },
    {
      scopes: [
        "constant",
        "constant.language",
        "constant.other",
        "support.constant",
        "variable.other.constant",
        "variable.other.enummember",
      ],
      foreground: palette.constant,
    },
    {
      scopes: ["entity.other.attribute-name", "meta.attribute", "variable.annotation"],
      foreground: palette.attribute,
    },
    {
      scopes: ["entity.name.type", "support.type", "support.class", "storage.type"],
      foreground: palette.type,
    },
    {
      scopes: ["keyword.operator"],
      foreground: palette.operator,
    },
    {
      scopes: [
        "entity.name.function",
        "support.function",
        "meta.function-call",
        "variable.function",
        "entity.name.function.macro",
        "markup.underline.link",
      ],
      foreground: palette.function,
    },
    {
      scopes: [
        "variable.other.property",
        "support.type.property-name",
        "support.variable.property",
        "meta.object-literal.key",
      ],
      foreground: palette.property,
    },
    {
      scopes: [
        "entity.name.tag",
        "variable.language.this",
        "variable.language.special",
        "variable.language.self",
      ],
      foreground: palette.tag,
    },
    {
      scopes: ["variable", "variable.other", "variable.parameter", "support.variable"],
      foreground: palette.variable,
    },
    {
      scopes: ["punctuation", "meta.brace"],
      foreground: palette.punctuation,
    },
    {
      scopes: ["entity.name.section", "entity.name.namespace", "markup.heading"],
      foreground: palette.function,
      fontStyle: "bold",
    },
    {
      scopes: ["meta.embedded"],
      foreground,
    },
  ];
}

export function buildSyntaxTheme(
  palette: SyntaxPalette,
  foreground: string,
  background: string,
  dark: boolean,
  name = THEME_NAME,
): ThemeRegistrationRaw {
  return {
    name,
    type: dark ? "dark" : "light",
    colors: {
      "editor.background": background,
      "editor.foreground": foreground,
    },
    settings: [
      { settings: { foreground, background } },
      ...syntaxRules(palette, foreground).map((rule) => ({
        scope: rule.scopes,
        settings: rule.fontStyle
          ? { foreground: rule.foreground, fontStyle: rule.fontStyle }
          : { foreground: rule.foreground },
      })),
    ],
  };
}

let version = 0;

export function syntaxThemeFrom(read: (property: string) => string): ThemeRegistrationRaw | undefined {
  const palette = syntaxPalette(read);
  const foreground = read(FOREGROUND_PROPERTY).trim();
  const background = read(BACKGROUND_PROPERTY).trim();
  if (!palette || !isHexColor(foreground) || !isHexColor(background)) return undefined;
  version += 1;
  return buildSyntaxTheme(
    palette,
    foreground,
    background,
    isDarkColor(background),
    `${THEME_NAME}-${version}`,
  );
}

export function readSyntaxTheme(root?: Element): ThemeRegistrationRaw | undefined {
  if (!root && typeof document === "undefined") return undefined;
  const element = root ?? document.documentElement;
  const style = getComputedStyle(element);
  return syntaxThemeFrom((property) => style.getPropertyValue(property));
}
