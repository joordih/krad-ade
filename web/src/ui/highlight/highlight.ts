import type {
  HighlighterCore,
  ThemedToken,
  ThemeRegistrationRaw,
} from "shiki/core";
import { LANGUAGE_LOADERS } from "./languages";

let instance: Promise<HighlighterCore> | undefined;
const loaded = new Set<string>();
const loading = new Map<string, Promise<boolean>>();

function core(): Promise<HighlighterCore> {
  instance ??= (async () => {
    const [{ createHighlighterCore }, { createOnigurumaEngine }] = await Promise.all([
      import("shiki/core"),
      import("shiki/engine/oniguruma"),
    ]);
    return createHighlighterCore({
      themes: [],
      langs: [],
      engine: createOnigurumaEngine(() => import("shiki/wasm")),
    });
  })().catch((error: unknown) => {
    instance = undefined;
    throw error;
  });
  return instance;
}

async function loadLanguage(
  highlighter: HighlighterCore,
  language: string,
  seen: Set<string>,
): Promise<boolean> {
  if (loaded.has(language)) return true;
  const loader = LANGUAGE_LOADERS[language];
  if (!loader || seen.has(language)) return false;
  seen.add(language);
  const module = await loader();
  const grammars = Array.isArray(module.default) ? module.default : [module.default];
  await highlighter.loadLanguage(grammars);
  loaded.add(language);
  for (const grammar of grammars) {
    for (const embedded of grammar.embeddedLangs ?? grammar.embeddedLanguages ?? []) {
      if (LANGUAGE_LOADERS[embedded]) await loadLanguage(highlighter, embedded, seen);
    }
  }
  return true;
}

function ensureLanguage(highlighter: HighlighterCore, language: string): Promise<boolean> {
  if (loaded.has(language)) return Promise.resolve(true);
  const pending = loading.get(language);
  if (pending) return pending;
  const started = loadLanguage(highlighter, language, new Set()).finally(() => loading.delete(language));
  loading.set(language, started);
  return started;
}

export async function highlightCode(
  code: string,
  language: string,
  theme: ThemeRegistrationRaw,
): Promise<ThemedToken[][] | undefined> {
  try {
    const highlighter = await core();
    if (!(await ensureLanguage(highlighter, language))) return undefined;
    return highlighter.codeToTokensBase(code, { lang: language, theme });
  } catch (error) {
    console.warn(error);
    return undefined;
  }
}
