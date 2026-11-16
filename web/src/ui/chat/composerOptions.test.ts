import { describe, expect, it } from "vitest";
import type { ChatState } from "../../protocol/ChatState";
import type { Effort } from "../../protocol/Effort";
import type { ListedModels } from "../../protocol/ListedModels";
import type { ModelOffer } from "../../protocol/ModelOffer";
import { chosenModel, effortChoices, effortLabel, modelLabel, modelOffers, permissionLabel } from "./composerOptions";

function offer(value: string, name: string, efforts: ModelOffer["efforts"] = ["low", "medium"]): ModelOffer {
  return {
    value,
    name,
    description: null,
    efforts,
    default_effort: null,
    thinking_off: false,
    source: null,
    context: null,
    recommended: false,
    aliases: [],
  };
}

function chatState(model: string | null, chosen: string | null, effort: Effort | null = null): ChatState {
  return { model, chosen_model: chosen, effort } as ChatState;
}

const listed: ListedModels[] = [
  {
    host: { kind: "local" },
    harness: "ClaudeCode",
    models: [offer("opus", "Opus", ["low", "high"]), offer("shared", "Shared")],
  },
  {
    host: { kind: "local" },
    harness: "Codex",
    models: [offer("gpt", "GPT"), offer("shared", "Shared again")],
  },
];

describe("composer model options", () => {
  it("keeps the harness's models once, in order", () => {
    expect(modelOffers(listed, "ClaudeCode").map((known) => known.value)).toEqual(["opus", "shared"]);
    expect(modelOffers(listed, "Codex").map((known) => known.value)).toEqual(["gpt", "shared"]);
  });

  it("falls back to every model when the harness lists nothing", () => {
    expect(modelOffers(listed, "Shell").map((known) => known.value)).toEqual(["opus", "shared", "gpt"]);
  });
});

describe("composer labels", () => {
  it("reads the picked model's name before the runtime's label", () => {
    const offers = modelOffers(listed, "ClaudeCode");
    expect(modelLabel(chatState("Claude Opus", "opus"), offers)).toBe("Opus");
    expect(modelLabel(chatState("Claude Opus", null), offers)).toBe("Claude Opus");
    expect(modelLabel(undefined, offers)).toBe("Default model");
  });

  it("reports the chosen id only when one was picked", () => {
    expect(chosenModel(chatState("Opus", "opus"))).toBe("opus");
    expect(chosenModel(chatState("Opus", null))).toBeNull();
  });

  it("names efforts and permission modes the way the desktop does", () => {
    expect(effortLabel("xhigh")).toBe("Extra high");
    expect(effortLabel(null)).toBe("Default");
    expect(permissionLabel("acceptEdits")).toBe("Accept edits");
    expect(permissionLabel(null)).toBe("Ask before edits");
  });
});

describe("composer effort levels", () => {
  it("uses the picked model's levels", () => {
    const offers = modelOffers(listed, "ClaudeCode");
    expect(effortChoices(chatState(null, "opus"), offers)).toEqual(["low", "high"]);
  });

  it("offers the common levels when the model is unknown", () => {
    const offers = modelOffers(listed, "ClaudeCode");
    expect(effortChoices(chatState(null, null), offers)).toEqual(["low", "medium", "high", "xhigh", "max"]);
    expect(effortChoices(chatState(null, "gone"), offers)).toEqual(["low", "medium", "high", "xhigh", "max"]);
  });
});
