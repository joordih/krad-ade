import type { DirEntry } from "../../protocol/DirEntry";

export type TreeRow = { entry: DirEntry; depth: number; expanded: boolean };

const MAX_TREE_DEPTH = 64;
const PATH_SEPARATOR = "/";
const TRAILING_SEPARATORS = /\/+$/;
const MIN_GUTTER_DIGITS = 2;

export function treeRows(
  root: string,
  listing: (dir: string) => DirEntry[] | undefined,
  expanded: ReadonlySet<string>,
): TreeRow[] {
  const rows: TreeRow[] = [];
  const seen = new Set<string>();
  const walk = (dir: string, depth: number): void => {
    if (depth > MAX_TREE_DEPTH || seen.has(dir)) return;
    seen.add(dir);
    for (const entry of listing(dir) ?? []) {
      const open = entry.kind === "directory" && expanded.has(entry.path);
      rows.push({ entry, depth, expanded: open });
      if (open) walk(entry.path, depth + 1);
    }
  };
  walk(root, 0);
  return rows;
}

export function relativePath(root: string, path: string): string {
  const prefix = root.endsWith(PATH_SEPARATOR) ? root : `${root}${PATH_SEPARATOR}`;
  return path.startsWith(prefix) ? path.slice(prefix.length) : path;
}

export function baseName(path: string): string {
  return path.slice(path.lastIndexOf(PATH_SEPARATOR) + 1);
}

export function folderLabel(path: string): string {
  return baseName(path.replace(TRAILING_SEPARATORS, "")) || PATH_SEPARATOR;
}

export function gutterDigits(lines: number): number {
  return Math.max(String(Math.max(Math.floor(lines), 0)).length, MIN_GUTTER_DIGITS);
}

export function toggled(expanded: ReadonlySet<string>, path: string): ReadonlySet<string> {
  const next = new Set(expanded);
  if (next.has(path)) next.delete(path);
  else next.add(path);
  return next;
}

export function openFolders(root: string, expanded: ReadonlySet<string>): string[] {
  return [...new Set([root, ...expanded])];
}

