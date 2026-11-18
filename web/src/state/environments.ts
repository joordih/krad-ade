import type { EnvironmentChange } from "../protocol/EnvironmentChange";
import type { EnvironmentData } from "../protocol/EnvironmentData";
import type { Session } from "../protocol/Session";

function upsert<T>(items: T[], index: number, item: T, same: (known: T) => boolean): T[] {
  const kept = items.filter((known) => !same(known));
  const at = Math.min(Math.max(index, 0), kept.length);
  return [...kept.slice(0, at), item, ...kept.slice(at)];
}

export function applyChange(environment: EnvironmentData, change: EnvironmentChange): EnvironmentData {
  switch (change.change) {
    case "node":
      return { ...environment, nodes: upsert(environment.nodes, change.index, change.node, (node) => node.id === change.node.id) };
    case "node_removed":
      return { ...environment, nodes: environment.nodes.filter((node) => node.id !== change.id) };
    case "group":
      return { ...environment, groups: upsert(environment.groups, change.index, change.group, (group) => group.id === change.group.id) };
    case "group_removed":
      return { ...environment, groups: environment.groups.filter((group) => group.id !== change.id) };
    case "session":
      return {
        ...environment,
        sessions: upsert(environment.sessions ?? [], change.index, change.session, (session) => session.id === change.session.id),
      };
    case "session_removed":
      return { ...environment, sessions: (environment.sessions ?? []).filter((session) => session.id !== change.id) };
    case "active_session":
      return { ...environment, active_session: change.session };
    case "next_id":
      return { ...environment, next_id: change.next };
    case "title":
      return { ...environment, title: change.title };
    case "host":
      return { ...environment, host: change.host };
    default:
      return environment;
  }
}

export function withEnvironment(environments: EnvironmentData[], opened: EnvironmentData): EnvironmentData[] {
  return [...environments.filter((known) => known.name !== opened.name), opened];
}

export function ordered(environments: EnvironmentData[], order: readonly string[]): EnvironmentData[] {
  return order
    .map((name) => environments.find((known) => known.name === name))
    .filter((known): known is EnvironmentData => known !== undefined);
}

export function environmentNamed(
  environments: readonly EnvironmentData[] | undefined,
  name: string,
): EnvironmentData | undefined {
  return environments?.find((known) => known.name === name);
}

export function sessionIn(environment: EnvironmentData | undefined, session: number): Session | undefined {
  return environment?.sessions?.find((known) => known.id === session);
}
