import { describe, expect, it } from "vitest";
import type { EnvironmentData } from "../protocol/EnvironmentData";
import type { Session } from "../protocol/Session";
import type { TerminalNode } from "../protocol/TerminalNode";
import { sessionOfTerminal, shownNode } from "./sessions";

function node(id: number, kind: TerminalNode["kind"] = "Terminal"): TerminalNode {
  return { id, title: `tab ${id}`, state: "Idle", text_scale: 1, kind };
}

const chatTabs: Session["tabs"] = {
  tabs: [{ id: 1, name: "Chat", arrangement: "Max", tiles: [{ id: 10, share: 1 }], main_ratio: 1 }],
  active: 1,
  next_tab: 2,
};
const terminalTabs: Session["tabs"] = {
  tabs: [{ id: 1, name: "Shell", arrangement: "Max", tiles: [{ id: 20, share: 1 }], main_ratio: 1 }],
  active: 1,
  next_tab: 2,
};

function session(id: number, tabs: Session["tabs"], lastChat: number | null): Session {
  return { id, group: null, name: `Session ${id}`, tabs, panes: { panes: [], root: { pane: 0 }, next_id: 1 }, last_chat: lastChat };
}

describe("shown tab", () => {
  const sessions = session(1, chatTabs, 10);
  const nodes = [node(20), node(10, "Chat")];

  it("follows the picked tab, then the last chat, then the first chat", () => {
    expect(shownNode(nodes, sessions, 20)?.id).toBe(20);
    expect(shownNode(nodes, sessions)?.id).toBe(10);
    expect(shownNode(nodes, session(1, terminalTabs, null))?.id).toBe(10);
    expect(shownNode([node(20)], session(1, terminalTabs, null))?.id).toBe(20);
    expect(shownNode([], sessions)).toBeUndefined();
  });
});

describe("the session of a terminal", () => {
  it("finds the session whose tabs hold it", () => {
    const environment = { name: "main", nodes: [], meta: {}, next_id: 30, groups: [], sessions: [session(1, chatTabs, null), session(2, terminalTabs, null)] } as unknown as EnvironmentData;
    expect(sessionOfTerminal(environment, 20)?.id).toBe(2);
    expect(sessionOfTerminal(environment, 10)?.id).toBe(1);
    expect(sessionOfTerminal(environment, 99)).toBeUndefined();
  });
});
