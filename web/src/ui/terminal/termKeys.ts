import type { GridSize } from "../../protocol/GridSize";

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

export const CTRL_KEY = { key: "ctrl", label: "Ctrl" } as const;

const CONTROL_UPPER_FIRST = 64;
const CONTROL_UPPER_LAST = 95;
const CONTROL_LOWER_FIRST = 97;
const CONTROL_LOWER_LAST = 122;
const CONTROL_LOWER_OFFSET = 96;
const DELETE = "\u007f";
const NUL = "\u0000";

export function controlOf(character: string): string {
  const code = character.charCodeAt(0);
  if (code >= CONTROL_UPPER_FIRST && code <= CONTROL_UPPER_LAST) return String.fromCharCode(code - CONTROL_UPPER_FIRST);
  if (code >= CONTROL_LOWER_FIRST && code <= CONTROL_LOWER_LAST) return String.fromCharCode(code - CONTROL_LOWER_OFFSET);
  if (character === "?") return DELETE;
  if (character === " ") return NUL;
  return character;
}

export function withStickyCtrl(data: string, sticky: boolean): string {
  const first = data[0];
  if (!sticky || first === undefined) return data;
  return controlOf(first) + data.slice(1);
}

export function gridOf(cols: number, rows: number, cellWidth: number, cellHeight: number): GridSize {
  return {
    cols: Math.max(1, Math.floor(cols)),
    rows: Math.max(1, Math.floor(rows)),
    cell_width_px: Math.max(0, cellWidth),
    cell_height_px: Math.max(0, cellHeight),
  };
}
