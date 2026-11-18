import { describe, expect, it } from "vitest";
import type { ChatState } from "../../../protocol/ChatState";
import type { ListedModels } from "../../../protocol/ListedModels";
import type { ModelOffer } from "../../../protocol/ModelOffer";
import { modelFacts, modelRows, offersFor, openingHarness, otherHarnessChoices, railHarnesses, railStep, steppedHarness, toggledPreferences, type ModelSourceInput } from "./models";

function offer(value: string, name: string, extra: Partial<ModelOffer> = {}): ModelOffer {
  return {
    value,
    name,
    description: null,
    efforts: ["low", "medium"],
    default_effort: null,
    thinking_off: false,
    source: null,
    context: null,
    recommended: false,
    aliases: [],
    ...extra,
  };
}

function chat(chosen: string | null, extra: Partial<ChatState> = {}): ChatState {
  return {
    harness: "ClaudeCode",
    model: "claude-default",
    chosen_model: chosen,
    effort: null,
    account: null,
    context: { tokens: null, window: null },
    ...extra,
  } as ChatState;
}

const listed: ListedModels[] = [
  {
    host: { kind: "local" },
    harness: "ClaudeCode",
    models: [offer("opus", "Opus", { aliases: ["claude-opus"] }), offer("sonnet", "Sonnet", { recommended: true, efforts: ["low", "high"] })],
  },
  { host: { kind: "local" }, harness: "Codex", models: [offer("gpt", "GPT")] },
];

const source: ModelSourceInput = { listed, host: { kind: "local" }, settingsModels: [], preferences: undefined };

describe("composer model facts", () => {
  it("offers only the chat harness's models on its host", () => {
    expect(offersFor(source, "ClaudeCode").map((known) => known.value)).toEqual(["opus", "sonnet"]);
    expect(offersFor(source, "Opencode")).toEqual([]);
    expect(offersFor({ listed, host: { kind: "ssh", destination: "box" } as never }, "Codex").map((known) => known.value)).toEqual(["gpt"]);
  });

  it("reads the recommended model until one is picked, and matches a pick by alias", () => {
    const fresh = modelFacts(chat(null), source);
    expect(fresh.label).toBe("Sonnet");
    expect(fresh.efforts).toEqual(["low", "high"]);
    expect(fresh.choices.find((choice) => choice.current)?.id).toBe("sonnet");
    const picked = modelFacts(chat("claude-opus"), source);
    expect(picked.label).toBe("Opus");
    expect(picked.missing).toBe(false);
  });

  it("flags a picked model that is no longer offered and lists it first", () => {
    const facts = modelFacts(chat("haiku"), source);
    expect(facts.missing).toBe(true);
    expect(facts.label).toBe("haiku");
    expect(facts.choices[0]).toMatchObject({ id: "haiku", missing: true, current: true });
    expect(facts.efforts).toEqual(["low", "medium", "high", "xhigh", "max"]);
  });

  it("falls back to the settings models and measures the context", () => {
    const facts = modelFacts(chat(null, { harness: "ClaudeCode", context: { tokens: 50_000, window: 200_000 } }), {
      ...source,
      listed: [],
      settingsModels: ["opus"],
    });
    expect(facts.label).toBe("claude-default");
    expect(facts.choices.map((choice) => choice.id)).toEqual(["opus"]);
    expect(facts.contextPercent).toBe(25);
    expect(facts.contextWindow).toBe(200_000);
  });

  it("offers no effort for a harness without levels", () => {
    expect(modelFacts(chat(null, { harness: "Opencode" }), { ...source, listed: [] }).offersEffort).toBe(false);
  });
});

describe("the model menu", () => {
  it("puts favorites first, then plans by order, providers and free models, with their sections", () => {
    const facts = modelFacts(chat(null), {
      ...source,
      listed: [
        {
          host: { kind: "local" },
          harness: "ClaudeCode",
          models: [
            offer("free", "Free one", { source: "free" }),
            offer("loose", "Loose"),
            offer("pro", "Pro one", { source: { plan: { name: "Pro", order: 1 } } }),
            offer("max", "Max one", { source: { plan: { name: "Max", order: 0 } } }),
            offer("router", "Router one", { source: { provider: "OpenRouter" } }),
            offer("fav", "Fav", { source: "free" }),
          ],
        },
      ],
      preferences: { favorites: { ClaudeCode: ["fav"] }, defaults: { ClaudeCode: "pro" } },
    });
    const rows = modelRows(facts.choices, "");
    expect(rows.map((row) => (row.kind === "section" ? `# ${row.label}` : row.choice.id))).toEqual([
      "# Favorites",
      "fav",
      "# Models",
      "loose",
      "# Max",
      "max",
      "# Pro",
      "pro",
      "# OpenRouter",
      "router",
      "# Free",
      "free",
    ]);
    expect(facts.choices.find((choice) => choice.id === "pro")?.isDefault).toBe(true);
    expect(modelRows(facts.choices, "router").map((row) => (row.kind === "model" ? row.choice.id : row.label))).toEqual([
      "OpenRouter",
      "router",
    ]);
  });
});

describe("models of other harnesses", () => {
  it("lists each harness's offers, or its settings models, marked for that harness", () => {
    const others = otherHarnessChoices({ ...source, preferences: { favorites: { Codex: ["gpt"] } } }, ["Codex", "ClaudeCode", "Opencode"], (harness) =>
      harness === "ClaudeCode" ? ["opus"] : ["ignored"],
    );
    expect(others.map((choice) => `${choice.harness}/${choice.id}`)).toEqual([
      "Codex/gpt",
      "ClaudeCode/opus",
      "ClaudeCode/sonnet",
      "Opencode/ignored",
    ]);
    expect(others[0]).toMatchObject({ favorite: true, current: false });
  });

  it("reads the Gateway's models from the Gateway", () => {
    expect(offersFor({ ...source, gateway: [offer("gw", "Gateway one")] }, "Gateway").map((known) => known.value)).toEqual(["gw"]);
  });

  it("opens the rail on the current model's harness and cycles through it", () => {
    const own = modelFacts(chat(null), source).choices;
    const all = [...otherHarnessChoices(source, ["Codex"], () => []), ...own];
    expect(railHarnesses(all)).toEqual(["Codex", "ClaudeCode"]);
    expect(openingHarness(all)).toBe("ClaudeCode");
    expect(steppedHarness(["ClaudeCode", "Codex", "Gateway"], "Gateway", true)).toBe("ClaudeCode");
    expect(steppedHarness(["ClaudeCode", "Codex", "Gateway"], "ClaudeCode", false)).toBe("Gateway");
    expect([railStep("Tab", false), railStep("Tab", true), railStep("ArrowRight", false), railStep("ArrowLeft", false), railStep("Enter", false)]).toEqual([
      true,
      false,
      true,
      false,
      undefined,
    ]);
  });
});

describe("model marks", () => {
  it("toggles a favorite and the new-chat default for one harness", () => {
    const starred = toggledPreferences(undefined, "ClaudeCode", "opus", "favorite");
    expect(starred.favorites?.ClaudeCode).toEqual(["opus"]);
    expect(toggledPreferences(starred, "ClaudeCode", "opus", "favorite").favorites?.ClaudeCode).toEqual([]);
    const preferred = toggledPreferences(starred, "ClaudeCode", "opus", "default");
    expect(preferred.defaults?.ClaudeCode).toBe("opus");
    expect(toggledPreferences(preferred, "ClaudeCode", "opus", "default").defaults?.ClaudeCode).toBeUndefined();
  });
});
