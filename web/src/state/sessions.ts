import type { AgentState } from "../protocol/AgentState";
import type { EnvironmentData } from "../protocol/EnvironmentData";
import type { Session } from "../protocol/Session";
import type { TerminalGroup } from "../protocol/TerminalGroup";
import type { TerminalNode } from "../protocol/TerminalNode";
import { LOCAL, runnerKey, type AppState, type RuntimeKey } from "./store";

export type SessionItem = {
  runtime: RuntimeKey;
  environment: string;
  session: Session;
  nodes: TerminalNode[];
  state: AgentState;
  cloud: boolean;
};

export type GroupItem = {
  environment: string;
  group: TerminalGroup | undefined;
  sessions: SessionItem[];
};

const STATE_WEIGHT: Record<AgentState, number> = { WaitingForInput: 3, Working: 2, Done: 1, Idle: 0 };

export function sessionNodes(environment: EnvironmentData, session: Session): TerminalNode[] {
  const ids = session.tabs.tabs.flatMap((tab) => tab.tiles.map((tile) => tile.id));
  return ids
    .map((id) => environment.nodes.find((node) => node.id === id))
    .filter((node): node is TerminalNode => node !== undefined);
}

export function strongest(nodes: TerminalNode[]): AgentState {
  return nodes.reduce<AgentState>((best, node) => (STATE_WEIGHT[node.state] > STATE_WEIGHT[best] ? node.state : best), "Idle");
}

function item(runtime: RuntimeKey, environment: EnvironmentData, session: Session, cloud: boolean): SessionItem {
  const nodes = sessionNodes(environment, session);
  return { runtime, environment: environment.name, session, nodes, state: strongest(nodes), cloud };
}

export function groupsOf(state: AppState, environment: string): GroupItem[] {
  const local = state.views[LOCAL]?.environments.find((known) => known.name === environment);
  if (!local) return [];
  const reached = new Set(
    Object.entries(state.views)
      .filter(([key, view]) => key !== LOCAL && view.snapshot)
      .map(([key]) => key),
  );
  const groups: GroupItem[] = local.groups.map((group) => ({ environment, group, sessions: [] }));
  const loose: GroupItem = { environment, group: undefined, sessions: [] };
  const groupOf = (id: number | null | undefined) => groups.find((known) => known.group?.id === id) ?? loose;
  for (const session of local.sessions ?? []) {
    if (session.cloud && reached.has(runnerKey(session.cloud.runner))) continue;
    groupOf(session.group).sessions.push(item(LOCAL, local, session, session.cloud != null));
  }
  for (const [key, view] of Object.entries(state.views)) {
    if (key === LOCAL || !view.snapshot) continue;
    for (const hosted of view.snapshot.hosted) {
      if (hosted.origin && hosted.origin.environment !== environment) continue;
      const runnerEnvironment = view.environments.find((known) => known.name === hosted.environment);
      const session = runnerEnvironment?.sessions?.find((known) => known.id === hosted.session);
      if (!runnerEnvironment || !session) continue;
      groupOf(hosted.origin?.group).sessions.push(item(key, runnerEnvironment, session, true));
    }
  }
  return loose.sessions.length > 0 ? [...groups, loose] : groups;
}

export function environmentNames(state: AppState): string[] {
  return (state.views[LOCAL]?.environments ?? []).map((environment) => environment.name);
}
