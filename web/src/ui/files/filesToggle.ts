export type ShortcutKey = Pick<KeyboardEvent, "code" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">;

const TOGGLE_CODE = "KeyB";
const MAC_PLATFORM = /mac|iphone|ipad/i;
const MAC_HINT = "⌥⌘B";
const OTHER_HINT = "Ctrl+Alt+B";
const OPEN_KEY = "krad-web-files-open";
const OPEN_VALUE = "1";

export function isMacPlatform(platform: string): boolean {
  return MAC_PLATFORM.test(platform);
}

export function isFilesToggle(event: ShortcutKey, mac: boolean): boolean {
  if (event.code !== TOGGLE_CODE || !event.altKey || event.shiftKey) return false;
  return mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
}

export function filesToggleHint(mac: boolean): string {
  return mac ? MAC_HINT : OTHER_HINT;
}

export function readFilesOpen(storage: Pick<Storage, "getItem">): boolean {
  return storage.getItem(OPEN_KEY) === OPEN_VALUE;
}

export function writeFilesOpen(storage: Pick<Storage, "setItem" | "removeItem">, open: boolean): void {
  if (open) storage.setItem(OPEN_KEY, OPEN_VALUE);
  else storage.removeItem(OPEN_KEY);
}


export type SideSection = "explorer" | "changes";

const SECTION_KEY = "krad-web-side-section";
const CHANGES_SECTION: SideSection = "changes";

export function readSideSection(storage: Pick<Storage, "getItem">): SideSection {
  return storage.getItem(SECTION_KEY) === CHANGES_SECTION ? CHANGES_SECTION : "explorer";
}

export function writeSideSection(storage: Pick<Storage, "setItem">, section: SideSection): void {
  storage.setItem(SECTION_KEY, section);
}
