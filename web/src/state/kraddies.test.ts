import { describe, expect, it } from "vitest";
import type { EnvironmentData } from "../protocol/EnvironmentData";
import type { KraddieState } from "../protocol/KraddieState";
import type { Session } from "../protocol/Session";
import { EMPTY_VIEW, type RuntimeView } from "./runtimeView";
import { kraddieGroups, kraddieSessionHidden } from "./kraddies";
import type { AppState } from "./store";

const tabs: Session["tabs"] = {
  tabs: [{ id: 1, name: "Chat", arrangement: "Max", tiles: [{ id: 1, share: 1 }], main_ratio: 1 }],
  active: 1,
  next_tab: 2,
};

function kraddie(sidebar: "on" | "off", folder: "on" | "off"): KraddieState {
  return {
    key: "kraddie",
    kraddie: {
      name: "Kraddie",
      design: {} as KraddieState["kraddie"]["design"],
      harness: "ClaudeCode",
      switch: "on",
      settings: { sessions: { sidebar, folder } } as KraddieState["kraddie"]["settings"],
    },
    automations: [],
    check_ins: { runs: [] },
    mind: {} as KraddieState["mind"],
    activity: { days: {} },
  };
}

function stateWith(session: KraddieState, kraddieId: string): { state: AppState; session: Session } {
  const stored: Session = {
    id: 1,
    group: 1,
    name: "Refunds",
    tabs,
    panes: { panes: [], root: { pane: 0 }, next_id: 1 },
    last_chat: null,
    memory: { kraddie: kraddieId, mentions: [] },
  };
  const environment = {
    name: "main",
    nodes: [{ id: 1, title: "chat", state: "Idle", text_scale: 1, kind: "Chat" }],
    meta: {},
    next_id: 2,
    title: null,
    groups: [{ id: 1, name: "repo", root: "/repo" }],
    sessions: [stored],
  } as unknown as EnvironmentData;
  const view: RuntimeView = {
    ...EMPTY_VIEW,
    snapshot: { kraddies: { kraddies: [session] } } as unknown as RuntimeView["snapshot"],
    environments: [environment],
  };
  return { state: { status: { status: "closed" }, views: { local: view } }, session: stored };
}

describe("kraddie sessions", () => {
  it("lists a Kraddie's session under it when its folder is on", () => {
    const { state, session } = stateWith(kraddie("on", "on"), "kraddie");
    expect(kraddieGroups(state, "main")).toHaveLength(1);
    expect(kraddieGroups(state, "main")[0]?.sessions[0]?.session.id).toBe(session.id);
    expect(kraddieSessionHidden(state, kraddieGroups(state, "main")[0]!.sessions[0]!)).toBe(false);
  });

  it("keeps the folder closed and hides the session when the Kraddie says so", () => {
    const { state, session } = stateWith(kraddie("off", "off"), "kraddie");
    expect(kraddieGroups(state, "main")).toHaveLength(0);
    const item = { runtime: "local", environment: "main", session, nodes: [], state: "Idle" as const, cloud: false };
    expect(kraddieSessionHidden(state, item)).toBe(true);
  });

  it("leaves a session that no Kraddie owns alone", () => {
    const { state, session } = stateWith(kraddie("off", "on"), "missing");
    expect(kraddieGroups(state, "main")).toHaveLength(0);
    const item = { runtime: "local", environment: "main", session, nodes: [], state: "Idle" as const, cloud: false };
    expect(kraddieSessionHidden(state, item)).toBe(false);
  });
});
