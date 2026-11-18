import { useEffect, useState } from "react";
import { filesToggleHint, isFilesToggle, isMacPlatform, readFilesOpen, writeFilesOpen } from "./filesToggle";

export type FilesPanel = { shown: boolean; hint: string; toggle: () => void; close: () => void };

const MAC = isMacPlatform(navigator.platform);

export function useFilesPanel(wide: boolean): FilesPanel {
  const [open, setOpen] = useState(() => readFilesOpen(localStorage));
  const [sheet, setSheet] = useState(false);

  const setWide = (next: boolean) => {
    writeFilesOpen(localStorage, next);
    setOpen(next);
  };
  const toggle = () => (wide ? setWide(!open) : setSheet((shown) => !shown));

  useEffect(() => {
    const pressed = (event: KeyboardEvent) => {
      if (!isFilesToggle(event, MAC)) return;
      event.preventDefault();
      toggle();
    };
    addEventListener("keydown", pressed);
    return () => removeEventListener("keydown", pressed);
  });

  return {
    shown: wide ? open : sheet,
    hint: filesToggleHint(MAC),
    toggle,
    close: () => (wide ? setWide(false) : setSheet(false)),
  };
}
