import type { FileOpening } from "../protocol/FileOpening";
import type { TerminalNode } from "../protocol/TerminalNode";

export type FileTab = { path: string; pinned: boolean };
export type FileTabs = { tabs: FileTab[]; shown: string | undefined };

export const NO_FILE_TABS: FileTabs = { tabs: [], shown: undefined };

const STORAGE_PREFIX = "krad-web-file-tabs:";
const KEY_SEPARATOR = "|";

export function openFileTab(state: FileTabs, path: string, opening: FileOpening): FileTabs {
  const pinned = opening === "pinned";
  if (state.tabs.some((tab) => tab.path === path)) {
    const tabs = pinned ? state.tabs.map((tab) => (tab.path === path ? { ...tab, pinned } : tab)) : state.tabs;
    return { tabs, shown: path };
  }
  const preview = state.tabs.findIndex((tab) => !tab.pinned);
  if (preview >= 0 && !pinned) {
    return { tabs: state.tabs.map((tab, index) => (index === preview ? { path, pinned } : tab)), shown: path };
  }
  return { tabs: [...state.tabs, { path, pinned }], shown: path };
}

export function pinFileTab(state: FileTabs, path: string): FileTabs {
  return { ...state, tabs: state.tabs.map((tab) => (tab.path === path ? { ...tab, pinned: true } : tab)) };
}

export function showFileTab(state: FileTabs, path: string): FileTabs {
  return state.tabs.some((tab) => tab.path === path) ? { ...state, shown: path } : state;
}

export function hideFileTabs(state: FileTabs): FileTabs {
  return state.shown === undefined ? state : { ...state, shown: undefined };
}

export function closeFileTab(state: FileTabs, path: string): FileTabs {
  const index = state.tabs.findIndex((tab) => tab.path === path);
  if (index < 0) return state;
  const tabs = state.tabs.filter((tab) => tab.path !== path);
  if (state.shown !== path) return { tabs, shown: state.shown };
  const neighbour = tabs[Math.min(index, tabs.length - 1)];
  return { tabs, shown: neighbour?.path };
}

export function runtimeFileNode(nodes: readonly TerminalNode[], path: string): TerminalNode | undefined {
  return nodes.find((node) => node.file?.path === path && (node.file.view ?? "edit") === "edit");
}

export function fileTabsKey(runtime: string, environment: string, session: number): string {
  return `${STORAGE_PREFIX}${runtime}${KEY_SEPARATOR}${environment}${KEY_SEPARATOR}${session}`;
}

function isFileTab(value: unknown): value is FileTab {
  if (typeof value !== "object" || value === null) return false;
  const tab = value as Record<string, unknown>;
  return typeof tab.path === "string" && typeof tab.pinned === "boolean";
}

export function parseFileTabs(text: string | null): FileTabs {
  if (!text) return NO_FILE_TABS;
  try {
    const value: unknown = JSON.parse(text);
    if (typeof value !== "object" || value === null) return NO_FILE_TABS;
    const { tabs, shown } = value as Record<string, unknown>;
    if (!Array.isArray(tabs)) return NO_FILE_TABS;
    const known = tabs.filter(isFileTab);
    const visible = typeof shown === "string" && known.some((tab) => tab.path === shown) ? shown : undefined;
    return { tabs: known, shown: visible };
  } catch {
    return NO_FILE_TABS;
  }
}

export function readFileTabs(storage: Pick<Storage, "getItem">, key: string): FileTabs {
  return parseFileTabs(storage.getItem(key));
}

export function writeFileTabs(storage: Pick<Storage, "setItem" | "removeItem">, key: string, state: FileTabs): void {
  if (state.tabs.length === 0) storage.removeItem(key);
  else storage.setItem(key, JSON.stringify(state));
}
