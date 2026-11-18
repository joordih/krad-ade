import { useEffect, useState } from "react";
import { onThemeReload } from "../../runtime/http";

const DARK_QUERY = "(prefers-color-scheme: dark)";
const DARK = "dark";
const LIGHT = "light";

function sheetIsDark(): boolean {
  if (typeof document === "undefined") return true;
  const scheme = getComputedStyle(document.documentElement).colorScheme;
  if (scheme.includes(DARK)) return true;
  if (scheme.includes(LIGHT)) return false;
  return matchMedia(DARK_QUERY).matches;
}

export function useSheetDark(): boolean {
  const [dark, setDark] = useState(sheetIsDark);
  useEffect(() => {
    const media = matchMedia(DARK_QUERY);
    const update = () => setDark(sheetIsDark());
    media.addEventListener("change", update);
    const off = onThemeReload(update);
    return () => {
      media.removeEventListener("change", update);
      off();
    };
  }, []);
  return dark;
}
