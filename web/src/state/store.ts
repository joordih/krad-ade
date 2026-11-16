import { useSyncExternalStore } from "react";
import type { AgentRef } from "../protocol/AgentRef";
import type { Command } from "../protocol/Command";
import type { Reply } from "../protocol/Reply";
import { RuntimeClient, type ConnectionStatus } from "../runtime/client";
import { reloadTheme } from "../runtime/http";
import { applyFeed, EMPTY_VIEW, prependPage, type RuntimeView } from "./runtimeView";

export const LOCAL = "local";
export type RuntimeKey = string;
const EARLIER_PAGE = 40;

export function runnerKey(runner: number): RuntimeKey {
  return `runner:${runner}`;
}

export type AppState = {
  status: ConnectionStatus;
  views: Record<RuntimeKey, RuntimeView>;
};

type Listener = () => void;

export class AppStore {
  private state: AppState = { status: { status: "connecting" }, views: { [LOCAL]: EMPTY_VIEW } };
  private readonly listeners = new Set<Listener>();
  private readonly runners = new Map<number, RuntimeClient>();

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
    if (key === LOCAL) return this.local;
    const runner = Number(key.slice(key.indexOf(":") + 1));
    return this.runners.get(runner);
  }

  send(key: RuntimeKey, command: Command): Promise<Reply> {
    const client = this.client(key);
    if (!client) return Promise.reject(new Error("that runtime is not reached"));
    return client.send(command);
  }

  async loadEarlier(key: RuntimeKey, chat: AgentRef, before: number): Promise<void> {
    const client = this.client(key);
    if (!client || before <= 0) return;
    const count = Math.min(EARLIER_PAGE, before);
    const answer = await client.ask({ area: "chat", verb: "page", chat, before, count });
    if (answer.kind !== "chat_page") return;
    this.setView(key, prependPage(this.view(key), chat, answer.from, answer.messages));
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
      if (state.state !== "ready" || this.runners.has(runner)) continue;
      const client = this.local.relay(runner);
      this.runners.set(runner, client);
      const key = runnerKey(runner);
      client.onFeed((feed) => this.setView(key, applyFeed(this.view(key), feed)));
      client.connect();
    }
  }
}

export function useApp(store: AppStore): AppState {
  return useSyncExternalStore(store.subscribe, store.snapshot);
}
