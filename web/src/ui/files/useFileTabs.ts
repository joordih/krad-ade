import { useState } from "react";
import type { FileOpening } from "../../protocol/FileOpening";
import {
  closeFileTab,
  fileTabsKey,
  hideFileTabs,
  NO_FILE_TABS,
  openFileTab,
  pinFileTab,
  readFileTabs,
  showFileTab,
  writeFileTabs,
  type FileTabs,
} from "../../state/fileTabs";
import type { SessionPlace } from "../../state/sessions";

export type FileTabsOf = FileTabs & {
  key: string | undefined;
  open: (path: string, opening: FileOpening) => void;
  pin: (path: string) => void;
  show: (path: string) => void;
  hide: () => void;
  close: (path: string) => void;
};

export function useFileTabs(place: SessionPlace | undefined): FileTabsOf {
  const key = place ? fileTabsKey(place.runtime, place.environment, place.session) : undefined;
  const [kept, setKept] = useState<Record<string, FileTabs>>({});
  const current = key ? (kept[key] ?? readFileTabs(localStorage, key)) : NO_FILE_TABS;

  const update = (change: (state: FileTabs) => FileTabs) => {
    if (!key) return;
    const next = change(current);
    if (next === current) return;
    writeFileTabs(localStorage, key, next);
    setKept((previous) => ({ ...previous, [key]: next }));
  };

  return {
    ...current,
    key,
    open: (path, opening) => update((state) => openFileTab(state, path, opening)),
    pin: (path) => update((state) => pinFileTab(state, path)),
    show: (path) => update((state) => showFileTab(state, path)),
    hide: () => update(hideFileTabs),
    close: (path) => update((state) => closeFileTab(state, path)),
  };
}
