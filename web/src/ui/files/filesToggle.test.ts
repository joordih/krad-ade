import { describe, expect, it } from "vitest";
import { filesToggleHint, isFilesToggle, isMacPlatform, readFilesOpen, readSideSection, writeFilesOpen, writeSideSection } from "./filesToggle";

const key = { code: "KeyB", metaKey: false, ctrlKey: false, altKey: true, shiftKey: false };

function memory() {
  const kept = new Map<string, string>();
  return {
    getItem: (name: string) => kept.get(name) ?? null,
    setItem: (name: string, value: string) => void kept.set(name, value),
    removeItem: (name: string) => void kept.delete(name),
  };
}

describe("the files panel", () => {
  it("toggles with the desktop's side panel keys: Cmd+Alt+B on macOS, Ctrl+Alt+B elsewhere", () => {
    expect(isFilesToggle({ ...key, metaKey: true }, true)).toBe(true);
    expect(isFilesToggle({ ...key, ctrlKey: true }, true)).toBe(false);
    expect(isFilesToggle({ ...key, ctrlKey: true }, false)).toBe(true);
    expect(isFilesToggle({ ...key, metaKey: true }, false)).toBe(false);
    expect(isFilesToggle({ ...key, metaKey: true, shiftKey: true }, true)).toBe(false);
    expect(isFilesToggle({ ...key, code: "KeyN", metaKey: true }, true)).toBe(false);
    expect(filesToggleHint(true)).toBe("⌥⌘B");
    expect(filesToggleHint(false)).toBe("Ctrl+Alt+B");
    expect(isMacPlatform("MacIntel")).toBe(true);
    expect(isMacPlatform("Linux x86_64")).toBe(false);
  });

  it("starts closed and remembers being open in this browser", () => {
    const storage = memory();
    expect(readFilesOpen(storage)).toBe(false);
    writeFilesOpen(storage, true);
    expect(readFilesOpen(storage)).toBe(true);
    writeFilesOpen(storage, false);
    expect(readFilesOpen(storage)).toBe(false);
  });

  it("keeps the side panel's section, Explorer by default", () => {
    const storage = memory();
    expect(readSideSection(storage)).toBe("explorer");
    writeSideSection(storage, "changes");
    expect(readSideSection(storage)).toBe("changes");
  });
});
