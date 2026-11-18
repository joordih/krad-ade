import { useEffect, useRef } from "react";
import type { FileOpening } from "../../protocol/FileOpening";
import { pinFileCommand } from "../../state/commands/sessions";
import { runtimeFileNode } from "../../state/fileTabs";
import { placedSession, sessionNodes, shownNode, type SessionPlace } from "../../state/sessions";
import type { AppState, AppStore } from "../../state/store";
import { FilesPanel } from "../files/FilesPanel";
import { useFilesPanel } from "../files/useFilesPanel";
import { useFileTabs } from "../files/useFileTabs";
import { sendCommand } from "../primitives/send";
import { SessionView } from "./SessionView";

const RECONNECTING = "Reconnecting…";

export function SessionArea({
  store,
  state,
  selected,
  picked,
  onPick,
  onBack,
  wide,
}: {
  store: AppStore;
  state: AppState;
  selected: SessionPlace | undefined;
  picked: number | undefined;
  onPick: (terminal: number | undefined) => void;
  onBack: () => void;
  wide: boolean;
}) {
  const files = useFilesPanel(wide);
  const fileTabs = useFileTabs(selected);
  const found = placedSession(state, selected);
  const connected = state.status.status === "open";
  const runtime = selected?.runtime;
  const environment = found?.environment;
  const group = environment?.groups.find((known) => known.id === found?.session.group);
  const nodes = environment && found ? sessionNodes(environment, found.session) : [];
  const active = found ? shownNode(nodes, found.session, picked) : undefined;
  const viewed = fileTabs.shown ?? active?.file?.path;
  const seen = useRef({ key: fileTabs.key, picked });

  useEffect(() => {
    const last = seen.current;
    seen.current = { key: fileTabs.key, picked };
    if (last.key === fileTabs.key && last.picked !== picked) fileTabs.hide();
  });

  function open(path: string, opening: FileOpening) {
    const node = runtimeFileNode(nodes, path);
    if (node && runtime && environment) {
      fileTabs.hide();
      onPick(node.id);
      if (opening === "pinned" && !node.file?.pinned) sendCommand(store, runtime, pinFileCommand(environment.name, node.id));
    } else {
      fileTabs.open(path, opening);
    }
    if (!wide) files.close();
  }

  return (
    <>
      <main className="main">
        {!connected && <div className="status-line">{RECONNECTING}</div>}
        <SessionView
          store={store}
          state={state}
          selected={selected}
          picked={picked}
          onPick={onPick}
          onBack={onBack}
          wide={wide}
          fileTabs={fileTabs}
          filesShown={files.shown}
          filesHint={files.hint}
          onFiles={files.toggle}
        />
      </main>
      {files.shown && runtime && environment && (
        <FilesPanel
          key={`${runtime}/${environment.name}/${group?.root ?? ""}`}
          store={store}
          state={state}
          runtime={runtime}
          environment={environment.name}
          host={environment.host}
          group={group}
          connected={connected}
          viewed={viewed}
          wide={wide}
          onOpen={open}
          onClose={files.close}
        />
      )}
    </>
  );
}
