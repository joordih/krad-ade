import type { AgentPreset } from "../../../protocol/AgentPreset";
import type { ChatState } from "../../../protocol/ChatState";
import type { Host } from "../../../protocol/Host";
import type { ModelOffer } from "../../../protocol/ModelOffer";
import type { Snapshot } from "../../../protocol/Snapshot";
import { detectedAgents, switchableHarnesses } from "../../../state/harnesses";
import { modelFacts, otherHarnessChoices, type ModelFacts } from "./models";

export type ChatModelPlace = {
  environment: string;
  host: Host | undefined;
  switchable: boolean;
  inCloud: boolean;
};

type ModelsSnapshot = Pick<Snapshot, "models" | "gateway" | "hosts" | "settings">;

const SETTINGS_MODELS_HARNESS: AgentPreset = "ClaudeCode";

function gatewayOffers(snapshot: ModelsSnapshot | undefined): ModelOffer[] {
  return snapshot?.gateway.state === "ready" ? snapshot.gateway.models.map((model) => model.offer) : [];
}

export function chatModelFacts(state: ChatState | undefined, snapshot: ModelsSnapshot | undefined, place: ChatModelPlace): ModelFacts {
  const settings = snapshot?.settings;
  const harness = state?.harness;
  const source = {
    listed: snapshot?.models.listed ?? [],
    host: place.host,
    gateway: gatewayOffers(snapshot),
    preferences: settings?.model_preferences,
  };
  const own = modelFacts(state, { ...source, settingsModels: harness ? (settings?.harness_models?.[harness] ?? []) : [] });
  if (!harness || !place.switchable) return own;
  const harnesses = switchableHarnesses(settings, {
    current: harness,
    detected: detectedAgents(snapshot?.hosts, place.environment, place.host),
    gateway: snapshot?.gateway,
    inCloud: place.inCloud,
  });
  const others = otherHarnessChoices(source, harnesses, (other) =>
    other === SETTINGS_MODELS_HARNESS ? (settings?.harness_models?.[other] ?? []) : [],
  );
  return { ...own, choices: [...own.choices, ...others] };
}
