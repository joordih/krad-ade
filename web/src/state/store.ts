import { useSyncExternalStore } from "react";
import type { AgentRef } from "../protocol/AgentRef";
import type { Answer } from "../protocol/Answer";
import type { Command } from "../protocol/Command";
import type { Reply } from "../protocol/Reply";
import { RuntimeClient, type ConnectionStatus } from "../runtime/client";
import { reloadTheme } from "../runtime/http";
import { pageCommand } from "./commands/chat";
import { listFolderCommand } from "./commands/files";
import { applyFeed, EMPTY_VIEW, prependPage, withFolderListing, type RuntimeView } from "./runtimeView";
import { LOCAL, runnerKey, type RuntimeKey } from "./runtimes";

export { LOCAL, runnerKey, type RuntimeKey } from "./runtimes";

const EARLIER_PAGE = 40;
const NOT_REACHED = "that runtime is not reached";

export type AppState = {
  status: ConnectionStatus;
  views: Record<RuntimeKey, RuntimeView>;
};

type Listener = () => void;

export class AppStore {
  private state: AppState = { status: { status: "connecting" }, views: { [LOCAL]: EMPTY_VIEW } };
  private readonly listeners = new Set<Listener>();
  private readonly runners = new Map<RuntimeKey, RuntimeClient>();

  constructor(readonly local: RuntimeClient) {
    local.onStatus((status) => this.set({ ...this.state, status }));
    local.onFeed((feed) => {
      if (feed.feed === "event" && feed.kind.type === "settings_changed") reloadTheme();
      this.setView(LOCAL, applyFeed(this.view(LOCAL), feed));
      this.reachRunners();
    });
  }

  start(): void {
    this.local.connect();
  }

  stop(): void {
    for (const runner of this.runners.values()) runner.close();
    this.local.close();
  }

  subscribe = (listener: Listener): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  snapshot = (): AppState => this.state;

  view(key: RuntimeKey): RuntimeView {
    return this.state.views[key] ?? EMPTY_VIEW;
  }

  client(key: RuntimeKey): RuntimeClient | undefined {
    return key === LOCAL ? this.local : this.runners.get(key);
  }

  send(key: RuntimeKey, command: Command): Promise<Reply> {
    const client = this.client(key);
    return client ? client.send(command) : Promise.reject(new Error(NOT_REACHED));
  }

  ask(key: RuntimeKey, command: Command): Promise<Answer> {
    const client = this.client(key);
    return client ? client.ask(command) : Promise.reject(new Error(NOT_REACHED));
  }

  async loadEarlier(key: RuntimeKey, chat: AgentRef, before: number): Promise<void> {
    if (before <= 0) return;
    const answer = await this.ask(key, pageCommand(chat, before, Math.min(EARLIER_PAGE, before)));
    if (answer.kind !== "chat_page") return;
    this.setView(key, prependPage(this.view(key), chat, answer.from, answer.messages));
  }

  async loadFolder(key: RuntimeKey, environment: string, dir: string): Promise<void> {
    const answer = await this.ask(key, listFolderCommand(environment, dir));
    if (answer.kind !== "entries") return;
    this.setView(key, withFolderListing(this.view(key), environment, dir, answer.entries));
  }

  private set(state: AppState): void {
    this.state = state;
    for (const listener of this.listeners) listener();
  }

  private setView(key: RuntimeKey, view: RuntimeView): void {
    this.set({ ...this.state, views: { ...this.state.views, [key]: view } });
  }

  private reachRunners(): void {
    const shown = this.view(LOCAL).snapshot?.runners.shown ?? [];
    for (const { runner, state } of shown) {
      const key = runnerKey(runner);
      if (state.state !== "ready" || this.runners.has(key)) continue;
      const client = this.local.relay(runner);
      this.runners.set(key, client);
      client.onFeed((feed) => this.setView(key, applyFeed(this.view(key), feed)));
      client.connect();
    }
  }
}

export function useApp(store: AppStore): AppState {
  return useSyncExternalStore(store.subscribe, store.snapshot);
}
