export type RuntimeKey = string;

export const LOCAL: RuntimeKey = "local";

const RUNNER_PREFIX = "runner:";

export function runnerKey(runner: number): RuntimeKey {
  return `${RUNNER_PREFIX}${runner}`;
}
