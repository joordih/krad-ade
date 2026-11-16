import { useEffect, useMemo, useState } from "react";
import type { AttentionTarget } from "../protocol/AttentionTarget";
import { RuntimeClient } from "../runtime/client";
import { registerWorker } from "../runtime/push";
import { nextSpot, spotsOf } from "../state/attention";
import { selectSessionCommand } from "../state/sessionEdits";
import { environmentNames, sessionNodes, sessionOfTerminal, shownNode } from "../state/sessions";
import { AppStore, useApp } from "../state/store";
import { Pairing } from "./Pairing";
import { sendCommand } from "./send";
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
    addEventListener("hashchange", moved);
    return () => {
      removeEventListener("popstate", moved);
      removeEventListener("hashchange", moved);
    };
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
  const [picked, setPicked] = useState<number>();

  useEffect(() => {
    registerWorker((url) => {
      location.hash = new URL(url).hash;
    }).catch((error: unknown) => console.warn(error));
  }, []);

  useEffect(() => {
    store.start();
    return () => store.stop();
  }, [store]);

  const spots = useMemo(
    () =>
      spotsOf(state.views).filter((spot) => {
        const known = state.views[spot.runtime]?.environments.find(
          (candidate) => candidate.name === spot.target.environment,
        );
        return known ? sessionOfTerminal(known, spot.target.terminal) !== undefined : false;
      }),
    [state.views],
  );

  const shown = useMemo(() => {
    if (!selected) return undefined;
    const view = state.views[selected.runtime];
    const known = view?.environments.find((candidate) => candidate.name === selected.environment);
    const session = known?.sessions?.find((candidate) => candidate.id === selected.session);
    if (!known || !session) return undefined;
    return shownNode(sessionNodes(known, session), session, picked);
  }, [selected, picked, state.views]);

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

  function open(next: Selected, terminal?: number) {
    setPicked(terminal);
    if (next) sendCommand(store, next.runtime, selectSessionCommand(next.environment, next.session));
    setSelected(next);
  }

  function jump() {
    const next = nextSpot(
      spots,
      selected && shown
        ? { runtime: selected.runtime, environment: selected.environment, terminal: shown.id }
        : undefined,
    );
    if (!next) return;
    const view = state.views[next.runtime];
    const known = view?.environments.find((candidate) => candidate.name === next.target.environment);
    const session = known ? sessionOfTerminal(known, next.target.terminal) : undefined;
    if (!session) {
      sendCommand(store, next.runtime, { area: "attention", verb: "dismiss", target: next.target });
      return;
    }
    setEnvironment(sidebarName(next.runtime, next.target.environment));
    open(
      { runtime: next.runtime, environment: next.target.environment, session: session.id },
      next.target.terminal,
    );
    markSeen(next.runtime, next.target);
  }

  /** The sidebar lists a cloud session under the environment it came from. */
  function sidebarName(runtime: string, name: string): string {
    if (names.includes(name)) return name;
    const hosted = state.views[runtime]?.snapshot?.hosted.find((entry) => entry.environment === name);
    return hosted?.origin?.environment ?? name;
  }

  function markSeen(runtime: string, target: AttentionTarget) {
    sendCommand(store, runtime, { area: "attention", verb: "seen", target });
  }

  return (
    <div className={`shell ${selected ? "at-session" : "at-list"}`}>
      {current && (
        <Sidebar
          store={store}
          state={state}
          environment={current}
          onEnvironment={(name) => {
            setEnvironment(name);
            open(undefined);
          }}
          selected={selected}
          onSelect={open}
          onJump={jump}
          attentionCount={spots.length}
          wide={wide}
        />
      )}
      <main className="main">
        {status.status !== "open" && <div className="status-line">Reconnecting…</div>}
        <SessionView
          store={store}
          state={state}
          selected={selected}
          picked={picked}
          onPick={setPicked}
          onBack={() => open(undefined)}
          wide={wide}
        />
      </main>
    </div>
  );
}
