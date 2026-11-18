import type { Answer } from "../../protocol/Answer";
import type { Unreadable } from "../../protocol/Unreadable";

export type FileRead =
  | { kind: "text"; text: string; len: number; truncated: boolean }
  | { kind: "error"; message: string };

const BYTE_UNITS = ["KB", "MB", "GB", "TB"];
const BYTES_PER_UNIT = 1024;
const ROUND_ABOVE = 10;
const TENTHS = 10;

export function formatBytes(bytes: number): string {
  if (bytes < BYTES_PER_UNIT) return `${Math.max(bytes, 0)} B`;
  let value = bytes / BYTES_PER_UNIT;
  let unit = 0;
  while (value >= BYTES_PER_UNIT && unit < BYTE_UNITS.length - 1) {
    value /= BYTES_PER_UNIT;
    unit += 1;
  }
  const rounded = value >= ROUND_ABOVE ? Math.round(value) : Math.round(value * TENTHS) / TENTHS;
  return `${rounded} ${BYTE_UNITS[unit]}`;
}

export function unreadableText(unreadable: Unreadable): string {
  switch (unreadable.reason) {
    case "too_large":
      return `This file is too large to show (${formatBytes(unreadable.len)}; the limit is ${formatBytes(unreadable.limit)}).`;
    case "not_text":
      return "This file is not text, so it cannot be shown here.";
    case "missing":
      return "This file no longer exists.";
    default:
      return "This file is outside the group's folder.";
  }
}

export function fileAnswer(answer: Answer): FileRead | undefined {
  if (answer.kind === "file_text") {
    return { kind: "text", text: answer.text, len: answer.len, truncated: answer.truncated };
  }
  if (answer.kind === "unreadable") return { kind: "error", message: unreadableText(answer.unreadable) };
  return undefined;
}
