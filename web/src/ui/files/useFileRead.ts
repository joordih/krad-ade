import { useEffect, useState } from "react";
import type { ThemedToken } from "shiki/core";
import { readFileCommand } from "../../state/commands/files";
import type { AppStore, RuntimeKey } from "../../state/store";
import { highlightCode } from "../highlight/highlight";
import { useSyntaxTheme } from "../highlight/useSyntaxTheme";
import { fileAnswer, type FileRead } from "./fileRead";

export type Loaded = { kind: "loading" } | { kind: "ready"; read: FileRead };

const MAX_HIGHLIGHT_SIZE = 400_000;
const NOT_SENT = "The runtime did not send the file's text.";
const CANT_OPEN = "Can't open this file.";

function failure(message: string): Loaded {
  return { kind: "ready", read: { kind: "error", message } };
}

export function useFileRead(store: AppStore, runtime: RuntimeKey, environment: string, path: string, reload: unknown): Loaded {
  const [loaded, setLoaded] = useState<Loaded>({ kind: "loading" });
  useEffect(() => {
    let live = true;
    setLoaded({ kind: "loading" });
    store
      .ask(runtime, readFileCommand(environment, path))
      .then((answer) => {
        if (live) setLoaded({ kind: "ready", read: fileAnswer(answer) ?? { kind: "error", message: NOT_SENT } });
      })
      .catch((error: unknown) => {
        if (live) setLoaded(failure(error instanceof Error ? error.message : CANT_OPEN));
      });
    return () => {
      live = false;
    };
  }, [store, runtime, environment, path, reload]);
  return loaded;
}

export function useHighlight(text: string | undefined, language: string | undefined): ThemedToken[][] | undefined {
  const theme = useSyntaxTheme();
  const [tokens, setTokens] = useState<ThemedToken[][]>();
  useEffect(() => {
    setTokens(undefined);
    if (text === undefined || theme === undefined || language === undefined || text.length > MAX_HIGHLIGHT_SIZE) return;
    let live = true;
    void highlightCode(text, language, theme).then((next) => {
      if (live && next) setTokens(next);
    });
    return () => {
      live = false;
    };
  }, [text, theme, language]);
  return tokens;
}
