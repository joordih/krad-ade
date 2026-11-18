import { useRef, useState } from "react";

export function useRename(onCommit: (draft: string) => void) {
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState("");
  const cancelled = useRef(false);
  return {
    renaming,
    draft,
    setDraft,
    start: (current: string) => {
      setDraft(current);
      cancelled.current = false;
      setRenaming(true);
    },
    cancel: () => {
      cancelled.current = true;
      setRenaming(false);
    },
    commit: () => {
      setRenaming(false);
      if (cancelled.current) {
        cancelled.current = false;
        return;
      }
      onCommit(draft);
    },
  };
}

export type Rename = ReturnType<typeof useRename>;

export function RenameField({ rename, label, className }: { rename: Rename; label: string; className: string }) {
  return (
    <input
      className={className}
      value={rename.draft}
      autoFocus
      aria-label={label}
      onChange={(event) => rename.setDraft(event.target.value)}
      onBlur={rename.commit}
      onKeyDown={(event) => {
        if (event.key === "Enter") event.currentTarget.blur();
        else if (event.key === "Escape") rename.cancel();
      }}
    />
  );
}
