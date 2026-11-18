import { Ghostty, type ITheme } from "ghostty-web";
import wasmUrl from "ghostty-web/ghostty-vt.wasm?url";

const ANSI_NAMES = [
  "black",
  "red",
  "green",
  "yellow",
  "blue",
  "magenta",
  "cyan",
  "white",
  "brightBlack",
  "brightRed",
  "brightGreen",
  "brightYellow",
  "brightBlue",
  "brightMagenta",
  "brightCyan",
  "brightWhite",
] as const;

const FALLBACK_FONT = "monospace";
const FALLBACK_FONT_SIZE = 13;
const TOKEN_PREFIX = "--krad-";

let loading: Promise<Ghostty> | undefined;

export function loadGhostty(): Promise<Ghostty> {
  loading ??= Ghostty.load(wasmUrl);
  return loading;
}

export type TerminalLook = { fontFamily: string; fontSize: number; theme: ITheme };

export function lookOf(element: HTMLElement): TerminalLook {
  const styles = getComputedStyle(element);
  const token = (name: string) => styles.getPropertyValue(`${TOKEN_PREFIX}${name}`).trim();
  const theme: ITheme = {
    background: token("bg-surface"),
    foreground: token("text-default"),
    cursor: token("accent"),
    cursorAccent: token("on-accent"),
    selectionBackground: token("bg-selected"),
  };
  ANSI_NAMES.forEach((name, index) => {
    theme[name] = token(`ansi-${index}`);
  });
  return {
    fontFamily: token("mono") || FALLBACK_FONT,
    fontSize: Number.parseFloat(token("text-sm")) || FALLBACK_FONT_SIZE,
    theme,
  };
}
