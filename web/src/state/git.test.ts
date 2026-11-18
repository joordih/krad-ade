import { describe, expect, it } from "vitest";
import type { GitRead } from "../protocol/GitRead";
import type { GroupGit } from "../protocol/GroupGit";
import { changedPath, changeFolder, changeMark, changeMarks, changeSections, changeTone, groupGit, hasRemote, NO_CHANGE_MARKS, totalLines, withGitRead } from "./git";

function read(root: string, remotes: boolean[]): GitRead {
  return {
    host: { kind: "local" },
    root,
    git: {
      repo_root: root,
      status: {} as never,
      lines: {},
      worktrees: [],
      refs: { branches: remotes.map((remote, at) => ({ name: `b${at}`, place: remote ? { place: "remote", remote: "origin" } : { place: "local" }, upstream: null, is_head: at === 0 })), tags: [] },
    },
  };
}

describe("git reads", () => {
  it("keeps the latest read of each root and sees its remote branches", () => {
    const reads = withGitRead(withGitRead([], read("/r", [false])), read("/r", [false, true]));
    expect(reads).toHaveLength(1);
    expect(hasRemote(reads, { kind: "local" }, "/r")).toBe(true);
    expect(hasRemote(reads, { kind: "local" }, "/other")).toBe(false);
    expect(hasRemote([read("/r", [false])], { kind: "local" }, "/r")).toBe(false);
  });
});

function changed(): GroupGit {
  const modified = { path: "src/ui/App.tsx", kind: "modified" as const, origin: null };
  const fresh = { path: "notes.md", kind: "untracked" as const, origin: null };
  return {
    repo_root: "/repo/",
    status: { head: null, branch: "main", upstream: null, ahead: 0, behind: 0, changes: [modified, fresh], staged: [modified], unstaged: [fresh] },
    lines: { "src/ui/App.tsx": { added: 3, removed: 1 }, "notes.md": { added: 2, removed: 0 } },
    worktrees: [],
    refs: { branches: [], tags: [] },
  };
}

describe("change marks", () => {
  it("letters and colours each kind of change as the desktop tree does", () => {
    expect(changeMark("modified")).toBe("M");
    expect(changeMark("untracked")).toBe("U");
    expect(changeMark("added")).toBe("A");
    expect(changeMark("deleted")).toBe("D");
    expect(changeMark("renamed")).toBe("R");
    expect(changeMark("conflicted")).toBe("!");
    expect(changeTone("renamed")).toBe("modified");
    expect(changeTone("untracked")).toBe("added");
    expect(changeTone("conflicted")).toBe("removed");
  });

  it("marks changed files by their full path and every folder above them", () => {
    const marks = changeMarks(changed());
    expect(marks.files.get("/repo/src/ui/App.tsx")).toBe("modified");
    expect(marks.files.get("/repo/notes.md")).toBe("untracked");
    expect(marks.folders.has("/repo/src/ui")).toBe(true);
    expect(marks.folders.has("/repo/src")).toBe(true);
    expect(marks.folders.has("/repo")).toBe(true);
    expect(marks.folders.has("/repo/src/ui/App.tsx")).toBe(false);
    expect(changeMarks(undefined)).toBe(NO_CHANGE_MARKS);
  });

  it("finds the group's read, sums its lines and splits staged from unstaged", () => {
    const git = changed();
    const reads: GitRead[] = [{ host: { kind: "local" }, root: "/repo", git }];
    expect(groupGit(reads, { kind: "local" }, "/repo")).toBe(git);
    expect(groupGit(reads, { kind: "local" }, "/else")).toBeUndefined();
    expect(totalLines(git)).toEqual({ added: 5, removed: 1 });
    expect(changeSections(git).map((section) => section.label)).toEqual(["Staged Changes", "Changes"]);
    expect(changedPath(git, git.status.unstaged[0]!)).toBe("/repo/notes.md");
    expect(changeFolder("src/ui/App.tsx")).toBe("src/ui");
    expect(changeFolder("notes.md")).toBe("");
  });
});
