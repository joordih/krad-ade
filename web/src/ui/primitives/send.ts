import type { Command } from "../../protocol/Command";
import type { AppStore, RuntimeKey } from "../../state/store";

export function warn(error: unknown): void {
  console.warn(error);
}

export function sendCommand(store: AppStore, runtime: RuntimeKey, command: Command | undefined): void {
  if (command) void store.send(runtime, command).catch(warn);
}

export function sendCommands(store: AppStore, runtime: RuntimeKey, commands: readonly Command[]): void {
  for (const command of commands) sendCommand(store, runtime, command);
}
