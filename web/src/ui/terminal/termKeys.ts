import type { GridSize } from "../../protocol/GridSize";

/** A key the phone keyboard has no way to send, with the bytes a terminal expects. */
export type ExtraKey = {
  key: string;
  label: string;
  data: string;
  icon?: string;
};

export const EXTRA_KEYS: ExtraKey[] = [
  { key: "escape", label: "Esc", data: "\u001b" },
  { key: "tab", label: "Tab", data: "\t" },
  { key: "up", label: "Up", data: "\u001b[A", icon: "arrow-up-01" },
  { key: "down", label: "Down", data: "\u001b[B", icon: "arrow-down-01" },
  { key: "left", label: "Left", data: "\u001b[D", icon: "arrow-left-01" },
  { key: "right", label: "Right", data: "\u001b[C", icon: "arrow-right-01" },
  { key: "ctrl-c", label: "Ctrl+C", data: "\u0003" },
];

/** The sticky modifier: the next typed character goes back as a control code. */
export const CTRL_KEY = { key: "ctrl", label: "Ctrl" } as const;

/**
 * The control code a character produces with Ctrl held, following the terminal
 * convention (`a`–`z` → 0x01–0x1a, `[` → Esc, `?` → Delete).
 */
export function controlOf(character: string): string {
  const code = character.charCodeAt(0);
  if (code >= 64 && code <= 95) return String.fromCharCode(code - 64);
  if (code >= 97 && code <= 122) return String.fromCharCode(code - 96);
  if (character === "?") return "\u007f";
  if (character === " ") return "\u0000";
  return character;
}

/** Turns typed text into what it sends while the sticky Ctrl key is on. */
export function withStickyCtrl(data: string, sticky: boolean): string {
  const first = data[0];
  if (!sticky || first === undefined) return data;
  return controlOf(first) + data.slice(1);
}

/** The grid description the runtime needs alongside a resize. */
export function gridOf(cols: number, rows: number, cellWidth: number, cellHeight: number): GridSize {
  return {
    cols: Math.max(1, Math.floor(cols)),
    rows: Math.max(1, Math.floor(rows)),
    cell_width_px: Math.max(0, cellWidth),
    cell_height_px: Math.max(0, cellHeight),
  };
}
