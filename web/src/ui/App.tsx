import { useEffect, useMemo, useState } from "react";
import { RuntimeClient } from "../runtime/client";
import { AppStore, useApp } from "../state/store";
import { environmentNames } from "../state/sessions";
import { Pairing } from "./Pairing";
import { SessionView } from "./SessionView";
import { Sidebar, type Selected } from "./Sidebar";

const BUILD = "krad-web-0.4.1";
const SESSION_FRAGMENT = "#s=";

function selectedFromAddress(): Selected {
  if (!location.hash.startsWith(SESSION_FRAGMENT)) return undefined;
  const [runtime, environment, session] = decodeURIComponent(location.hash.slice(SESSION_FRAGMENT.length)).split("|");
  const id = Number(session);
  return runtime && environment && Number.isFinite(id) ? { runtime, environment, session: id } : undefined;
}

function addressOf(selected: Selected): string {
  if (!selected) return location.pathname;
  return `${SESSION_FRAGMENT}${encodeURIComponent([selected.runtime, selected.environment, selected.session].join("|"))}`;
}

function useSelected(): [Selected, (selected: Selected) => void] {
  const [selected, setSelected] = useState<Selected>(selectedFromAddress);
  useEffect(() => {
    const moved = () => setSelected(selectedFromAddress());
    addEventListener("popstate", moved);
    return () => removeEventListener("popstate", moved);
  }, []);
  const select = (next: Selected) => {
    if (next) {
      history.pushState(null, "", addressOf(next));
    } else if (selectedFromAddress()) {
      history.back();
      return;
    }
    setSelected(next);
  };
  return [selected, select];
}
const WIDE_QUERY = "(min-width: 900px)";

function useWide(): boolean {
  const [wide, setWide] = useState(() => matchMedia(WIDE_QUERY).matches);
  useEffect(() => {
    const media = matchMedia(WIDE_QUERY);
    const changed = () => setWide(media.matches);
    media.addEventListener("change", changed);
    return () => media.removeEventListener("change", changed);
  }, []);
  return wide;
}

export function App() {
  const store = useMemo(() => new AppStore(new RuntimeClient({ build: BUILD })), []);
  const state = useApp(store);
  const wide = useWide();
  const [environment, setEnvironment] = useState<string>();
  const [selected, setSelected] = useSelected();

  useEffect(() => {
    store.start();
    return () => store.stop();
  }, [store]);

  const status = state.status;
  if (status.status === "refused" || location.hash.startsWith("#pair=")) {
    return <Pairing refused={status.status === "refused" ? status.message : undefined} />;
  }
  if (status.status !== "open" && !state.views.local?.snapshot) {
    return <div className="empty">{status.status === "waiting" ? "Reconnecting…" : "Connecting…"}</div>;
  }
  const names = environmentNames(state);
  const current =
    environment && names.includes(environment) ? environment : (selected?.runtime === "local" ? selected.environment : undefined) ?? names[0];
  return (
    <div className={`shell ${selected ? "at-session" : "at-list"}`}>
      {current && (
        <Sidebar
          state={state}
          environment={current}
          onEnvironment={(name) => {
            setEnvironment(name);
            if (selected) setSelected(undefined);
          }}
          selected={selected}
          onSelect={setSelected}
        />
      )}
      <main className="main">
        {status.status !== "open" && <div className="status-line">Reconnecting…</div>}
        <SessionView store={store} state={state} selected={selected} onBack={() => setSelected(undefined)} wide={wide} />
      </main>
    </div>
  );
}
