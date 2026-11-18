import { describe, expect, it } from "vitest";
import type { DirEntry } from "../protocol/DirEntry";
import {
  applyFolderChanged,
  diagnosticsKey,
  diagnosticsOf,
  folderKey,
  listingOf,
  withFileDiagnostics,
  withListing,
} from "./files";

function entry(path: string): DirEntry {
  return { name: path.slice(path.lastIndexOf("/") + 1), path, kind: "file" };
}

describe("file listings", () => {
  it("keys listings and diagnostics by environment as well as path", () => {
    expect(folderKey("main", "/r")).not.toBe(folderKey("other", "/r"));
    expect(diagnosticsKey("main", "/r/a.rs")).not.toBe(diagnosticsKey("main", "/r/b.rs"));
    const listings = withListing({}, "main", "/r", [entry("/r/a.rs")]);
    expect(listingOf(listings, "main", "/r")).toHaveLength(1);
    expect(listingOf(listings, "other", "/r")).toBeUndefined();
  });

  it("replaces a watched folder with the listing its event carries", () => {
    const listings = withListing({}, "main", "/r", [entry("/r/a.rs")]);
    const changed = applyFolderChanged(listings, { environment: "main", dir: "/r", entries: [entry("/r/b.rs")] });
    expect(listingOf(changed, "main", "/r")?.map((known) => known.name)).toEqual(["b.rs"]);
  });
});

describe("file diagnostics", () => {
  it("replaces a file's diagnostics and reads them back", () => {
    const map = withFileDiagnostics({}, {
      environment: "main",
      path: "/r/a.rs",
      diagnostics: [
        {
          range: { start: { line: 1, character: 0 }, end: { line: 1, character: 1 } },
          severity: "warning",
          message: "problem",
          source: null,
        },
      ],
    });
    expect(diagnosticsOf(map, "main", "/r/a.rs")).toHaveLength(1);
    expect(diagnosticsOf(map, "main", "/r/b.rs")).toEqual([]);
  });
});
