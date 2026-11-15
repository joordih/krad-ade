import type { Event } from "../protocol/Event";
import type { Feed } from "../protocol/Feed";
import type { Snapshot } from "../protocol/Snapshot";

export type RuntimeState = {
  snapshot: Snapshot | undefined;
  seq: number | undefined;
};

export const EMPTY: RuntimeState = { snapshot: undefined, seq: undefined };

export type Reducer = (state: RuntimeState, event: Event) => RuntimeState;

export function applyFeed(state: RuntimeState, feed: Feed, reduce: Reducer): RuntimeState {
  if (feed.feed === "snapshot") {
    const { feed: _feed, ...snapshot } = feed;
    return { snapshot, seq: snapshot.seq };
  }
  const { feed: _feed, ...event } = feed;
  if (state.seq !== undefined && event.seq <= state.seq) return state;
  return { ...reduce(state, event), seq: event.seq };
}

export const reduceEvent: Reducer = (state, event) => {
  const snapshot = state.snapshot;
  if (!snapshot) return state;
  const kind = event.kind;
  switch (kind.type) {
    case "settings_changed":
      return { ...state, snapshot: { ...snapshot, settings: kind.settings } };
    case "buffer_opened": {
      const { type: _type, ...opened } = kind;
      const others = snapshot.buffers.filter((buffer) => buffer.buffer !== opened.buffer);
      return { ...state, snapshot: { ...snapshot, buffers: [...others, opened] } };
    }
    case "buffer_closed":
      return {
        ...state,
        snapshot: {
          ...snapshot,
          buffers: snapshot.buffers.filter((buffer) => buffer.buffer !== kind.buffer),
        },
      };
    default:
      return state;
  }
};
