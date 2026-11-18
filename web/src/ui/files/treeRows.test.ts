import { describe, expect, it } from "vitest";
import type { DirEntry } from "../../protocol/DirEntry";
import { baseName, folderLabel, gutterDigits, openFolders, relativePath, toggled, treeRows } from "./treeRows";

function entry(path: string, kind: DirEntry["kind"] = "file"): DirEntry {
  return { name: path.slice(path.lastIndexOf("/") + 1), path, kind };
}

describe("the file tree", () => {
  it("opens expanded folders right below their row and guards against loops", () => {
    const listings: Record<string, DirEntry[]> = {
      "/r": [entry("/r/src", "directory"), entry("/r/README.md")],
      "/r/src": [entry("/r/src/main.rs"), entry("/r/src/lib.rs")],
    };
    const listing = (dir: string) => listings[dir];
    expect(treeRows("/r", listing, new Set()).map((row) => row.entry.name)).toEqual(["src", "README.md"]);
    const rows = treeRows("/r", listing, new Set(["/r/src"]));
    expect(rows.map((row) => [row.entry.name, row.depth, row.expanded])).toEqual([
      ["src", 0, true],
      ["main.rs", 1, false],
      ["lib.rs", 1, false],
      ["README.md", 0, false],
    ]);
    expect(treeRows("/r", () => [entry("/r", "directory")], new Set(["/r"]))).toHaveLength(1);
  });

  it("shows paths below the root without their prefix", () => {
    expect(relativePath("/r", "/r/src/main.rs")).toBe("src/main.rs");
    expect(relativePath("/r/", "/r/src/main.rs")).toBe("src/main.rs");
    expect(relativePath("/r", "/other/main.rs")).toBe("/other/main.rs");
    expect(baseName("/r/src/main.rs")).toBe("main.rs");
    expect(baseName("LICENSE")).toBe("LICENSE");
  });

  it("counts gutter digits like the desktop", () => {
    expect(gutterDigits(7)).toBe(2);
    expect(gutterDigits(999)).toBe(3);
    expect(gutterDigits(12000)).toBe(5);
  });

  it("toggles a folder and lists the root with every open folder once", () => {
    const open = toggled(new Set(), "/r/src");
    expect([...open]).toEqual(["/r/src"]);
    expect([...toggled(open, "/r/src")]).toEqual([]);
    expect(openFolders("/r", new Set(["/r/src", "/r"]))).toEqual(["/r", "/r/src"]);
  });

  it("names a folder by its last part, and the disk root by a slash", () => {
    expect(folderLabel("/home/me/krad-ade")).toBe("krad-ade");
    expect(folderLabel("/home/me/krad-ade/")).toBe("krad-ade");
    expect(folderLabel("/")).toBe("/");
  });
});
