import type { CloudRunner } from "../../../protocol/CloudRunner";
import type { Host } from "../../../protocol/Host";
import type { RunnerShown } from "../../../protocol/RunnerShown";
import type { RunnerState } from "../../../protocol/RunnerState";

export type RunOn = { kind: "here" } | { kind: "runner"; runner: number };

export type RunOnChoice = { runOn: RunOn; label: string; state: string | undefined; open: boolean };

export const HERE: RunOn = { kind: "here" };

const LOCAL_LABEL = "This computer";
const UNKNOWN_LABEL = "Unknown";
const CLOSED_HINT = "Reconnect it in Settings to use it";
const DETAIL_SEPARATOR = " · ";

const STATE_LABEL: Record<RunnerState["state"], string> = {
  ready: "Ready",
  update_pending: "Update pending",
  connecting: "Connecting",
  signing_in: "Signing in",
  installing: "Installing",
  not_connected: "Not connected",
  failed: "Failed",
};

export function hostLabel(host: Host | undefined): string {
  if (!host || host.kind === "local") return LOCAL_LABEL;
  return host.kind === "ssh" ? host.destination : host.machine.label;
}

export function sameRunOn(left: RunOn, right: RunOn): boolean {
  return left.kind === right.kind && (left.kind === "here" || (right.kind === "runner" && left.runner === right.runner));
}

export function runOnChoices(here: string, runners: readonly CloudRunner[], shown: readonly RunnerShown[]): RunOnChoice[] {
  const remote = runners.map((runner): RunOnChoice => {
    const state = shown.find((known) => known.runner === runner.id)?.state;
    return {
      runOn: { kind: "runner", runner: runner.id },
      label: runner.name,
      state: state ? STATE_LABEL[state.state] : UNKNOWN_LABEL,
      open: state !== undefined && state.state !== "failed",
    };
  });
  return [{ runOn: HERE, label: here, state: undefined, open: true }, ...remote];
}

export function choiceDetail(choice: RunOnChoice): string | undefined {
  if (!choice.state) return undefined;
  return choice.open ? choice.state : `${choice.state}${DETAIL_SEPARATOR}${CLOSED_HINT}`;
}

export function runOnOffered(offer: { runners: number; fresh: boolean; cloudSession: boolean; remote: boolean }): boolean {
  return offer.runners > 0 && offer.fresh && !offer.cloudSession && offer.remote;
}
