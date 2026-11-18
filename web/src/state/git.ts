import type { ChangeKind } from "../protocol/ChangeKind";
import type { FileChange } from "../protocol/FileChange";
import type { GitRead } from "../protocol/GitRead";
import type { GroupGit } from "../protocol/GroupGit";
import type { Host } from "../protocol/Host";
import type { LineCount } from "../protocol/LineCount";

export function sameHost(left: Host, right: Host): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function sameRead(left: GitRead, right: GitRead): boolean {
  return left.root === right.root && sameHost(left.host, right.host);
}

export function withGitRead(reads: readonly GitRead[], read: GitRead): GitRead[] {
  return [...reads.filter((known) => !sameRead(known, read)), read];
}

export function hasRemote(reads: readonly GitRead[], host: Host, root: string): boolean {
  const read = reads.find((known) => known.root === root && sameHost(known.host, host));
  return read?.git?.refs.branches.some((branch) => branch.place.place === "remote") ?? false;
}

export type ChangeTone = "modified" | "added" | "removed";
export type ChangeMarks = { files: ReadonlyMap<string, ChangeKind>; folders: ReadonlySet<string> };

const PATH_SEPARATOR = "/";
const TRAILING_SEPARATORS = /\/+$/;

const CHANGE_MARK: Record<ChangeKind, string> = {
  modified: "M",
  added: "A",
  deleted: "D",
  renamed: "R",
  untracked: "U",
  conflicted: "!",
};

const CHANGE_TONE: Record<ChangeKind, ChangeTone> = {
  modified: "modified",
  renamed: "modified",
  added: "added",
  untracked: "added",
  deleted: "removed",
  conflicted: "removed",
};

export const NO_CHANGE_MARKS: ChangeMarks = { files: new Map(), folders: new Set() };

export function changeMark(kind: ChangeKind): string {
  return CHANGE_MARK[kind];
}

export function changeTone(kind: ChangeKind): ChangeTone {
  return CHANGE_TONE[kind];
}

export function groupGit(reads: readonly GitRead[], host: Host, root: string): GroupGit | undefined {
  return reads.find((known) => known.root === root && sameHost(known.host, host))?.git ?? undefined;
}

function joined(root: string, path: string): string {
  return `${root.replace(TRAILING_SEPARATORS, "")}${PATH_SEPARATOR}${path}`;
}

function ancestors(path: string): string[] {
  const found: string[] = [];
  for (let end = path.lastIndexOf(PATH_SEPARATOR); end >= 0; end = path.lastIndexOf(PATH_SEPARATOR, end - 1)) {
    found.push(end === 0 ? PATH_SEPARATOR : path.slice(0, end));
    if (end === 0) break;
  }
  return found;
}

export function changeMarks(git: GroupGit | undefined): ChangeMarks {
  if (!git) return NO_CHANGE_MARKS;
  const files = new Map<string, ChangeKind>();
  const folders = new Set<string>();
  for (const change of git.status.changes) {
    const path = joined(git.repo_root, change.path);
    files.set(path, change.kind);
    for (const folder of ancestors(path)) folders.add(folder);
  }
  return { files, folders };
}

export function totalLines(git: GroupGit): LineCount {
  return Object.values(git.lines).reduce(
    (total, lines) => ({ added: total.added + lines.added, removed: total.removed + lines.removed }),
    { added: 0, removed: 0 },
  );
}

export type ChangeSection = { label: string; changes: FileChange[] };

const STAGED_LABEL = "Staged Changes";
const UNSTAGED_LABEL = "Changes";

export function changeSections(git: GroupGit): ChangeSection[] {
  return [
    { label: STAGED_LABEL, changes: git.status.staged },
    { label: UNSTAGED_LABEL, changes: git.status.unstaged },
  ].filter((section) => section.changes.length > 0);
}

export function changedPath(git: GroupGit, change: FileChange): string {
  return joined(git.repo_root, change.path);
}

export function changeFolder(path: string): string {
  const end = path.lastIndexOf(PATH_SEPARATOR);
  return end < 0 ? "" : path.slice(0, end);
}
