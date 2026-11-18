import { useEffect, useMemo, useState } from "react";
import type { AttentionTarget } from "../protocol/AttentionTarget";
import { RuntimeClient } from "../runtime/client";
import { registerWorker } from "../runtime/push";
import { selectSessionCommand } from "../state/commands/sessions";
import { nextJump, reachableSpots } from "../state/jump";
import { environmentNames, placedSession, sessionNodes, shownNode, type SessionPlace } from "../state/sessions";
import { AppStore, LOCAL, useApp, type RuntimeKey } from "../state/store";
import { Pairing } from "./pairing/Pairing";
import { sendCommand, warn } from "./primitives/send";
import { SessionArea } from "./session/SessionArea";
import { pairingCodeFromHash } from "./shell/address";
import { useSelected } from "./shell/useSelected";
import { useWide } from "./shell/useWide";
import { Sidebar } from "./sidebar/Sidebar";

const BUILD = `krad-web-${__KRAD_WEB_VERSION__}`;
const CONNECTING = "Connecting…";
const RECONNECTING = "Reconnecting…";

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
    }).catch(warn);
  }, []);

  useEffect(() => {
    store.start();
    return () => store.stop();
  }, [store]);

  const spots = useMemo(() => reachableSpots(state), [state]);

  const shown = useMemo(() => {
    const found = placedSession(state, selected);
    return found && shownNode(sessionNodes(found.environment, found.session), found.session, picked);
  }, [selected, picked, state]);

  const status = state.status;
  if (status.status === "refused" || pairingCodeFromHash(location.hash) !== undefined) {
    return <Pairing refused={status.status === "refused" ? status.message : undefined} />;
  }
  if (status.status !== "open" && !state.views[LOCAL]?.snapshot) {
    return <div className="empty">{status.status === "waiting" ? RECONNECTING : CONNECTING}</div>;
  }
  const names = environmentNames(state);
  const fromSelection = selected?.runtime === LOCAL ? selected.environment : undefined;
  const current = environment && names.includes(environment) ? environment : (fromSelection ?? names[0]);

  function open(next: SessionPlace | undefined, terminal?: number) {
    setPicked(terminal);
    if (next) sendCommand(store, next.runtime, selectSessionCommand(next.environment, next.session));
    setSelected(next);
  }

  function markSeen(runtime: RuntimeKey, target: AttentionTarget) {
    sendCommand(store, runtime, { area: "attention", verb: "seen", target });
  }

  function jump() {
    const here = selected && shown ? { runtime: selected.runtime, environment: selected.environment, terminal: shown.id } : undefined;
    const next = nextJump(state, spots, here);
    if (!next) return;
    if (next.kind === "dismiss") {
      sendCommand(store, next.spot.runtime, { area: "attention", verb: "dismiss", target: next.spot.target });
      return;
    }
    setEnvironment(next.listedIn);
    open(next.place, next.spot.target.terminal);
    markSeen(next.spot.runtime, next.spot.target);
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
      <SessionArea
        store={store}
        state={state}
        selected={selected}
        picked={picked}
        onPick={setPicked}
        onBack={() => open(undefined)}
        wide={wide}
      />
    </div>
  );
}
