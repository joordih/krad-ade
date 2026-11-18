import { countIn } from "../../state/attention";
import type { SessionItem, SessionPlace } from "../../state/sessions";
import type { AppState, AppStore } from "../../state/store";
import { SessionRow } from "./SessionRow";

function isAt(place: SessionPlace | undefined, item: SessionItem): boolean {
  return place?.runtime === item.runtime && place.environment === item.environment && place.session === item.session.id;
}

function placeOf(item: SessionItem): SessionPlace {
  return { runtime: item.runtime, environment: item.environment, session: item.session.id };
}

export function SessionList({
  store,
  state,
  items,
  selected,
  wide,
  indent = false,
  onSelect,
}: {
  store: AppStore;
  state: AppState;
  items: readonly SessionItem[];
  selected: SessionPlace | undefined;
  wide: boolean;
  indent?: boolean;
  onSelect: (place: SessionPlace | undefined) => void;
}) {
  return items.map((item) => {
    const waiting = state.views[item.runtime]?.attention.waiting ?? [];
    return (
      <SessionRow
        key={`${item.runtime}/${item.environment}/${item.session.id}`}
        store={store}
        item={item}
        selected={isAt(selected, item)}
        waiting={countIn(waiting, item.environment, item.nodes.map((node) => node.id))}
        settings={state.views[item.runtime]?.snapshot?.settings}
        wide={wide}
        indent={indent}
        onSelect={() => onSelect(placeOf(item))}
        onClosed={() => {
          if (isAt(selected, item)) onSelect(undefined);
        }}
      />
    );
  });
}
