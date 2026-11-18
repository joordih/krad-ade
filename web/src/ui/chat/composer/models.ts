import type { AgentPreset } from "../../../protocol/AgentPreset";
import type { ChatState } from "../../../protocol/ChatState";
import type { ContextSize } from "../../../protocol/ContextSize";
import type { Effort } from "../../../protocol/Effort";
import type { Host } from "../../../protocol/Host";
import type { ListedModels } from "../../../protocol/ListedModels";
import type { ModelOffer } from "../../../protocol/ModelOffer";
import type { ModelPreferences } from "../../../protocol/ModelPreferences";
import type { ModelSource } from "../../../protocol/ModelSource";
import { sameHost } from "../../../state/git";

export type ModelChoice = {
  id: string;
  harness: AgentPreset;
  label: string;
  current: boolean;
  section: ModelSource | null;
  description: string | null;
  recommended: boolean;
  context: ContextSize | null;
  missing: boolean;
  favorite: boolean;
  isDefault: boolean;
};

export type ModelFacts = {
  harness: AgentPreset | undefined;
  label: string;
  missing: boolean;
  choices: ModelChoice[];
  effort: Effort | null;
  efforts: Effort[];
  defaultEffort: Effort | null;
  offersEffort: boolean;
  account: string | null;
  contextPercent: number | undefined;
  contextWindow: number | null;
};

export type ModelSourceInput = {
  listed: readonly ListedModels[];
  host: Host | undefined;
  gateway?: readonly ModelOffer[];
  settingsModels: readonly string[];
  preferences: ModelPreferences | undefined;
};

export type ModelRow = { kind: "section"; label: string } | { kind: "model"; choice: ModelChoice };

export const CLAUDE_EFFORTS: readonly Effort[] = ["low", "medium", "high", "xhigh", "max"];
const MODEL_FALLBACK = "Model";
const FAVORITES_LABEL = "Favorites";
const MODELS_LABEL = "Models";
const FREE_LABEL = "Free";
const CONTEXT_LABEL: Record<ContextSize, string> = { one_million: "1M context" };
const FULL_PERCENT = 100;
const NO_RANK = -1;
const PROVIDER_RANK = Number.MAX_SAFE_INTEGER - 1;
const FREE_RANK = Number.MAX_SAFE_INTEGER;

function answersTo(offer: ModelOffer, value: string): boolean {
  return offer.value === value || offer.aliases.includes(value);
}

export function harnessEfforts(harness: AgentPreset | undefined): readonly Effort[] {
  return harness === "ClaudeCode" ? CLAUDE_EFFORTS : [];
}

export function offersFor(source: Pick<ModelSourceInput, "listed" | "host" | "gateway">, harness: AgentPreset | undefined): ModelOffer[] {
  if (!harness) return [];
  if (harness === "Gateway" && source.gateway?.length) return [...source.gateway];
  const own = source.listed.filter((group) => group.harness === harness);
  const host = source.host;
  const here = host ? own.filter((group) => sameHost(group.host, host)) : own;
  const seen = new Set<string>();
  return (here.length > 0 ? here : own)
    .flatMap((group) => group.models)
    .filter((offer) => !seen.has(offer.value) && seen.add(offer.value));
}

type BareChoice = Omit<ModelChoice, "harness" | "favorite" | "isDefault">;

function choiceOf(offer: ModelOffer, current: boolean): BareChoice {
  return {
    id: offer.value,
    label: offer.name,
    current,
    section: offer.source,
    description: offer.description,
    recommended: offer.recommended,
    context: offer.context,
    missing: false,
  };
}

function plainChoice(id: string, current: boolean, missing: boolean): BareChoice {
  return { id, label: id, current, section: null, description: null, recommended: false, context: null, missing };
}

function contextPercent(state: ChatState | undefined): number | undefined {
  const tokens = state?.context.tokens;
  const window = state?.context.window;
  if (tokens == null || !window) return undefined;
  return Math.min(Math.round((tokens / window) * FULL_PERCENT), FULL_PERCENT);
}

export function modelFacts(state: ChatState | undefined, source: ModelSourceInput): ModelFacts {
  const harness = state?.harness;
  const offers = offersFor(source, harness);
  const chosen = state?.chosen_model ?? null;
  const shown = chosen
    ? offers.find((offer) => answersTo(offer, chosen))
    : (offers.find((offer) => offer.recommended) ?? offers[0]);
  const missing = chosen !== null && offers.length > 0 && shown === undefined;
  const base =
    offers.length > 0
      ? offers.map((offer) => choiceOf(offer, offer === shown))
      : source.settingsModels.map((id) => plainChoice(id, id === chosen, false));
  const all = missing && chosen ? [plainChoice(chosen, true, true), ...base] : base;
  return {
    harness,
    label: shown?.name ?? (missing ? chosen : null) ?? state?.model ?? MODEL_FALLBACK,
    missing,
    choices: harness ? marked(all, harness, source.preferences) : [],
    effort: state?.effort ?? null,
    efforts: shown ? shown.efforts : [...harnessEfforts(harness)],
    defaultEffort: shown?.default_effort ?? null,
    offersEffort: harnessEfforts(harness).length > 0 || offers.some((offer) => offer.efforts.length > 0),
    account: state?.account ?? null,
    contextPercent: contextPercent(state),
    contextWindow: state?.context.window ?? null,
  };
}

function marked(choices: readonly BareChoice[], harness: AgentPreset, preferences: ModelPreferences | undefined): ModelChoice[] {
  const favorites = preferences?.favorites?.[harness] ?? [];
  const preferred = preferences?.defaults?.[harness];
  return choices.map((choice) => ({ ...choice, harness, favorite: favorites.includes(choice.id), isDefault: preferred === choice.id }));
}

export function otherHarnessChoices(
  source: Omit<ModelSourceInput, "settingsModels">,
  harnesses: readonly AgentPreset[],
  settingsModelsOf: (harness: AgentPreset) => readonly string[],
): ModelChoice[] {
  return harnesses.flatMap((harness) => {
    const offers = offersFor(source, harness);
    const base = offers.length > 0 ? offers.map((offer) => choiceOf(offer, false)) : settingsModelsOf(harness).map((id) => plainChoice(id, false, false));
    return marked(base, harness, source.preferences);
  });
}

export function railHarnesses(choices: readonly ModelChoice[]): AgentPreset[] {
  return [...new Set(choices.map((choice) => choice.harness))];
}

export function openingHarness(choices: readonly ModelChoice[]): AgentPreset | undefined {
  return (choices.find((choice) => choice.current) ?? choices[0])?.harness;
}

const RAIL_KEYS: Record<string, boolean> = { ArrowRight: true, ArrowLeft: false };

export function railStep(key: string, shiftKey: boolean): boolean | undefined {
  return key === "Tab" ? !shiftKey : RAIL_KEYS[key];
}

export function steppedHarness(rail: readonly AgentPreset[], shown: AgentPreset, forward: boolean): AgentPreset {
  const at = rail.indexOf(shown);
  if (at < 0 || rail.length === 0) return shown;
  return rail[(at + (forward ? 1 : rail.length - 1)) % rail.length] ?? shown;
}

function rankOf(section: ModelSource | null): number {
  if (section === null) return NO_RANK;
  if (section === "free") return FREE_RANK;
  return "plan" in section ? section.plan.order : PROVIDER_RANK;
}

function sectionLabel(section: ModelSource): string {
  if (section === "free") return FREE_LABEL;
  return "plan" in section ? section.plan.name : section.provider;
}

type Shelf = { key: string; label: string | undefined };

const LOOSE_SHELF = "";

function shelfOf(choice: ModelChoice): Shelf {
  if (choice.favorite) return { key: FAVORITES_LABEL, label: FAVORITES_LABEL };
  if (!choice.section) return { key: LOOSE_SHELF, label: undefined };
  const label = sectionLabel(choice.section);
  return { key: label, label };
}

function matches(choice: ModelChoice, query: string): boolean {
  return [choice.label, choice.description ?? "", choice.id].some((text) => text.toLowerCase().includes(query));
}

export function modelRows(choices: readonly ModelChoice[], query: string): ModelRow[] {
  const wanted = query.trim().toLowerCase();
  const shown = choices
    .map((choice, index) => ({ choice, index }))
    .filter(({ choice }) => !wanted || matches(choice, wanted))
    .sort(
      (left, right) =>
        Number(!left.choice.favorite) - Number(!right.choice.favorite) ||
        rankOf(left.choice.section) - rankOf(right.choice.section) ||
        left.index - right.index,
    )
    .map(({ choice }) => choice);
  const rows: ModelRow[] = [];
  let previous: string | undefined;
  for (const choice of shown) {
    const shelf = shelfOf(choice);
    const label = shelf.label ?? (previous === undefined ? undefined : MODELS_LABEL);
    if (shelf.key !== previous && label) rows.push({ kind: "section", label });
    previous = shelf.key;
    rows.push({ kind: "model", choice });
  }
  return rows;
}

export function contextLabel(context: ContextSize): string {
  return CONTEXT_LABEL[context];
}

export type ModelMark = "favorite" | "default";

export function toggledPreferences(
  preferences: ModelPreferences | undefined,
  harness: AgentPreset,
  model: string,
  mark: ModelMark,
): ModelPreferences {
  const known = preferences ?? {};
  if (mark === "default") {
    const defaults = { ...known.defaults };
    if (defaults[harness] === model) delete defaults[harness];
    else defaults[harness] = model;
    return { ...known, defaults };
  }
  const favorites = known.favorites?.[harness] ?? [];
  const next = favorites.includes(model) ? favorites.filter((kept) => kept !== model) : [...favorites, model];
  return { ...known, favorites: { ...known.favorites, [harness]: next } };
}
