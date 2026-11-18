import { describe, expect, it } from "vitest";
import type { TerminalNode } from "../protocol/TerminalNode";
import {
  closeFileTab,
  fileTabsKey,
  hideFileTabs,
  NO_FILE_TABS,
  openFileTab,
  parseFileTabs,
  pinFileTab,
  runtimeFileNode,
  showFileTab,
} from "./fileTabs";

describe("file tabs", () => {
  it("opens a file as a shown preview tab", () => {
    expect(openFileTab(NO_FILE_TABS, "/r/a.ts", "preview")).toEqual({ tabs: [{ path: "/r/a.ts", pinned: false }], shown: "/r/a.ts" });
  });

  it("replaces the preview tab in place with the next previewed file", () => {
    const pinned = openFileTab(NO_FILE_TABS, "/r/p.ts", "pinned");
    const previewed = openFileTab(pinned, "/r/a.ts", "preview");
    const replaced = openFileTab(previewed, "/r/b.ts", "preview");
    expect(replaced.tabs).toEqual([
      { path: "/r/p.ts", pinned: true },
      { path: "/r/b.ts", pinned: false },
    ]);
    expect(replaced.shown).toBe("/r/b.ts");
  });

  it("adds a pinned tab beside the preview one", () => {
    const state = openFileTab(openFileTab(NO_FILE_TABS, "/r/a.ts", "preview"), "/r/b.ts", "pinned");
    expect(state.tabs).toEqual([
      { path: "/r/a.ts", pinned: false },
      { path: "/r/b.ts", pinned: true },
    ]);
  });

  it("pins a preview tab by double click or by opening it pinned", () => {
    const previewed = openFileTab(NO_FILE_TABS, "/r/a.ts", "preview");
    expect(pinFileTab(previewed, "/r/a.ts").tabs).toEqual([{ path: "/r/a.ts", pinned: true }]);
    expect(openFileTab(previewed, "/r/a.ts", "pinned").tabs).toEqual([{ path: "/r/a.ts", pinned: true }]);
    const kept = openFileTab(pinFileTab(previewed, "/r/a.ts"), "/r/b.ts", "preview");
    expect(kept.tabs).toHaveLength(2);
  });

  it("focuses a file that is already open instead of opening it twice", () => {
    const state = openFileTab(openFileTab(NO_FILE_TABS, "/r/a.ts", "pinned"), "/r/b.ts", "pinned");
    const focused = openFileTab(hideFileTabs(state), "/r/a.ts", "preview");
    expect(focused.tabs).toEqual(state.tabs);
    expect(focused.shown).toBe("/r/a.ts");
    expect(focused.tabs[0]?.pinned).toBe(true);
  });

  it("shows the neighbour when the shown tab closes and the chat after the last one", () => {
    const state = openFileTab(openFileTab(openFileTab(NO_FILE_TABS, "/r/a", "pinned"), "/r/b", "pinned"), "/r/c", "pinned");
    const middle = closeFileTab(showFileTab(state, "/r/b"), "/r/b");
    expect(middle.tabs.map((tab) => tab.path)).toEqual(["/r/a", "/r/c"]);
    expect(middle.shown).toBe("/r/c");
    expect(closeFileTab(state, "/r/a").shown).toBe("/r/c");
    expect(closeFileTab(openFileTab(NO_FILE_TABS, "/r/a", "preview"), "/r/a")).toEqual(NO_FILE_TABS);
    expect(closeFileTab(hideFileTabs(state), "/r/c").shown).toBeUndefined();
  });

  it("finds a file the runtime already has open as a tab", () => {
    const node = { id: 4, file: { path: "/r/a.ts", pinned: true } } as TerminalNode;
    const changes = { id: 5, file: { path: "/r/b.ts", pinned: true, view: "changes" } } as TerminalNode;
    expect(runtimeFileNode([node, changes], "/r/a.ts")).toBe(node);
    expect(runtimeFileNode([node, changes], "/r/b.ts")).toBeUndefined();
  });

  it("reads back what it kept and ignores anything else", () => {
    const state = openFileTab(NO_FILE_TABS, "/r/a.ts", "pinned");
    expect(parseFileTabs(JSON.stringify(state))).toEqual(state);
    expect(parseFileTabs(JSON.stringify({ tabs: [{ path: 1 }], shown: "/x" }))).toEqual(NO_FILE_TABS);
    expect(parseFileTabs("{")).toEqual(NO_FILE_TABS);
    expect(parseFileTabs(null)).toEqual(NO_FILE_TABS);
    expect(fileTabsKey("local", "Work", 3)).not.toBe(fileTabsKey("local", "Work", 4));
  });
});
