import type { AgentPreset } from "../../protocol/AgentPreset";
import type { ChatState } from "../../protocol/ChatState";
import type { Effort } from "../../protocol/Effort";
import type { ListedModels } from "../../protocol/ListedModels";
import type { ModelOffer } from "../../protocol/ModelOffer";
import type { PermissionMode } from "../../protocol/PermissionMode";

/** The modes the desktop offers in the composer, in its order. */
export const PERMISSION_MODES: PermissionMode[] = ["manual", "acceptEdits", "plan", "auto"];

/** Used when no offered model description tells us which levels exist. */
export const EVERY_EFFORT: Effort[] = ["low", "medium", "high", "xhigh", "max"];

const PERMISSION_LABEL: Record<PermissionMode, string> = {
  manual: "Ask before edits",
  acceptEdits: "Accept edits",
  plan: "Plan",
  auto: "Auto",
  dontAsk: "Don't ask",
  bypassPermissions: "Bypass permissions",
};

const EFFORT_LABEL: Record<Effort, string> = {
  minimal: "Minimal",
  low: "Low",
  medium: "Medium",
  high: "High",
  xhigh: "Extra high",
  max: "Max",
  ultra: "Ultra",
};

export function permissionLabel(mode: PermissionMode | null | undefined): string {
  return mode ? PERMISSION_LABEL[mode] : PERMISSION_LABEL.manual;
}

export function effortLabel(effort: Effort | null | undefined): string {
  return effort ? EFFORT_LABEL[effort] : "Default";
}

/**
 * The models this chat's harness offers. Falls back to every listed model so a
 * chat whose harness has no catalogue yet still gets a usable picker.
 */
export function modelOffers(listed: readonly ListedModels[], harness: AgentPreset | undefined): ModelOffer[] {
  const all = listed.flatMap((group) => group.models);
  const matching = harness ? listed.filter((group) => group.harness === harness).flatMap((group) => group.models) : all;
  const source = matching.length > 0 ? matching : all;
  const seen = new Set<string>();
  return source.filter((offer) => {
    if (seen.has(offer.value)) return false;
    seen.add(offer.value);
    return true;
  });
}

/** The id the runtime knows the chat's model by, when the person picked one. */
export function chosenModel(state: ChatState | undefined): string | null {
  return state?.chosen_model ?? null;
}

/** What the composer pill reads: the picked model's name, else the runtime's label. */
export function modelLabel(state: ChatState | undefined, offers: readonly ModelOffer[]): string {
  const chosen = offers.find((offer) => offer.value === chosenModel(state));
  return chosen?.name ?? state?.model ?? "Default model";
}

/** The effort levels the picked model accepts; the common set when it is unknown. */
export function effortChoices(state: ChatState | undefined, offers: readonly ModelOffer[]): Effort[] {
  const chosen = offers.find((offer) => offer.value === chosenModel(state));
  return chosen && chosen.efforts.length > 0 ? chosen.efforts : EVERY_EFFORT;
}
