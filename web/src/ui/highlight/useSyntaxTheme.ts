import { useEffect, useState } from "react";
import type { ThemeRegistrationRaw } from "shiki/core";
import { onThemeReload } from "../../runtime/http";
import { readSyntaxTheme } from "./theme";

export function useSyntaxTheme(): ThemeRegistrationRaw | undefined {
  const [theme, setTheme] = useState<ThemeRegistrationRaw | undefined>(() => readSyntaxTheme());
  useEffect(() => {
    const update = () => setTheme(readSyntaxTheme());
    update();
    addEventListener("load", update);
    const off = onThemeReload(update);
    return () => {
      removeEventListener("load", update);
      off();
    };
  }, []);
  return theme;
}
