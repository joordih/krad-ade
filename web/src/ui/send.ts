import type { Command } from "../protocol/Command";
import type { AppStore, RuntimeKey } from "../state/store";

/** Sends a command to the runtime that owns it; a refusal only reaches the console. */
export function sendCommand(store: AppStore, runtime: RuntimeKey, command: Command): void {
  void store.send(runtime, command).catch((error: unknown) => console.warn(error));
}
