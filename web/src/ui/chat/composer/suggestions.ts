export type SuggestKind = "mention" | "skill";

export type SuggestTrigger = { kind: SuggestKind; start: number; end: number; query: string };

const MENTION_KEY = "@";
const SKILL_KEY = "/";
const WORD_GAP = " ";
const FOLDER_SUFFIX = "/";
const WHITESPACE = /\s/;

export function suggestTrigger(text: string, cursor: number): SuggestTrigger | undefined {
  const before = text.slice(0, cursor);
  let start = before.length;
  while (start > 0 && !WHITESPACE.test(before.charAt(start - 1))) start -= 1;
  const word = before.slice(start);
  const kind: SuggestKind | undefined = word.startsWith(MENTION_KEY) ? "mention" : word.startsWith(SKILL_KEY) ? "skill" : undefined;
  return kind ? { kind, start, end: cursor, query: word.slice(1) } : undefined;
}

export function mentionText(path: string, folder: boolean): string {
  return folder ? `${path}${FOLDER_SUFFIX}` : path;
}

export function accepted(text: string, trigger: SuggestTrigger, value: string): { text: string; cursor: number } {
  const key = trigger.kind === "mention" ? MENTION_KEY : SKILL_KEY;
  const inserted = `${key}${value}${WORD_GAP}`;
  return { text: text.slice(0, trigger.start) + inserted + text.slice(trigger.end), cursor: trigger.start + inserted.length };
}

export function typedKey(text: string, cursor: number, key: SuggestKind): { text: string; cursor: number } {
  const typed = key === "mention" ? MENTION_KEY : SKILL_KEY;
  const gap = cursor > 0 && !WHITESPACE.test(text.charAt(cursor - 1)) ? WORD_GAP : "";
  const inserted = `${gap}${typed}`;
  return { text: text.slice(0, cursor) + inserted + text.slice(cursor), cursor: cursor + inserted.length };
}

export function entryLabels(path: string): { name: string; folder: string } {
  const at = path.lastIndexOf(FOLDER_SUFFIX);
  return at < 0 ? { name: path, folder: "" } : { name: path.slice(at + 1), folder: path.slice(0, at) };
}

export function matchingSkills<T extends { name: string; description: string }>(skills: readonly T[], query: string): T[] {
  const wanted = query.toLowerCase();
  return skills.filter((skill) => skill.name.toLowerCase().includes(wanted) || skill.description.toLowerCase().includes(wanted));
}
