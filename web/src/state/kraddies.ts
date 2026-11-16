import type { KraddieId } from "../protocol/KraddieId";
import type { KraddieState } from "../protocol/KraddieState";
import type { RuntimeView } from "./runtimeView";
import { groupsOf, type SessionItem } from "./sessions";
import type { AppState } from "./store";

/** Sessions a Kraddie runs, grouped under it in the sidebar. */
export type KraddieGroup = {
  id: KraddieId;
  name: string;
  sessions: SessionItem[];
};

function idOf(state: KraddieState): string {
  return state.key ?? state.kraddie.name;
}

/** The Kraddie that owns a session, when this runtime knows it. */
export function kraddieStateOf(
  view: RuntimeView | undefined,
  id: KraddieId | null | undefined,
): KraddieState | undefined {
  if (!id) return undefined;
  return view?.snapshot?.kraddies.kraddies.find((state) => idOf(state) === id);
}

function kraddieOf(state: AppState, item: SessionItem): KraddieState | undefined {
  return kraddieStateOf(state.views[item.runtime], item.session.memory?.kraddie);
}

/** Whether the desktop hides this session from its group: its Kraddie keeps sessions out of the sidebar. */
export function kraddieSessionHidden(state: AppState, item: SessionItem): boolean {
  const kraddie = kraddieOf(state, item);
  return kraddie?.kraddie.settings?.sessions.sidebar === "off";
}

/** Sessions whose Kraddie lists them in its folder, newest runtime state first. */
export function kraddieGroups(state: AppState, environment: string): KraddieGroup[] {
  const groups = new Map<string, KraddieGroup>();
  for (const group of groupsOf(state, environment)) {
    for (const item of group.sessions) {
      const kraddie = kraddieOf(state, item);
      if (!kraddie || kraddie.kraddie.settings?.sessions.folder !== "on") continue;
      const id = idOf(kraddie);
      const known = groups.get(id);
      if (known) {
        known.sessions.push(item);
      } else {
        groups.set(id, { id, name: kraddie.kraddie.name, sessions: [item] });
      }
    }
  }
  return [...groups.values()];
}
