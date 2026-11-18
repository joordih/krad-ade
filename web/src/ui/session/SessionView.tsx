import type { AgentPreset } from "../../protocol/AgentPreset";
import { closeNodeCommands, newChatCommands, newTerminalCommands, pinFileCommand, renameNodeCommand, selectSessionCommand } from "../../state/commands/sessions";
import { defaultChatHarness, harnessLabel } from "../../state/harnesses";
import { newResumeId } from "../../state/ids";
import { placedSession, sessionNodes, shownNode, type SessionPlace } from "../../state/sessions";
import type { AppState, AppStore } from "../../state/store";
import { ChatView } from "../chat/ChatView";
import { FileTabBody } from "../files/FileTabBody";
import type { FileTabsOf } from "../files/useFileTabs";
import { useMenu } from "../primitives/Menu";
import { sendCommand, sendCommands } from "../primitives/send";
import { TerminalView } from "../terminal/TerminalView";
import { NewTabMenu } from "./NewTabMenu";
import { TabStrip } from "./TabStrip";

export function SessionView({
  store,
  state,
  selected,
  picked,
  onPick,
  onBack,
  wide,
  fileTabs,
  filesShown,
  filesHint,
  onFiles,
}: {
  store: AppStore;
  state: AppState;
  selected: SessionPlace | undefined;
  picked: number | undefined;
  onPick: (terminal: number | undefined) => void;
  onBack: () => void;
  wide: boolean;
  fileTabs: FileTabsOf;
  filesShown: boolean;
  filesHint: string;
  onFiles: () => void;
}) {
  const add = useMenu();
  const found = placedSession(state, selected);
  if (!selected || !found) {
    return <div className="empty">{selected ? "Opening this session…" : "Pick a session."}</div>;
  }
  const { environment, session } = found;
  const { runtime } = selected;
  const name = environment.name;
  const settings = state.views[runtime]?.snapshot?.settings;
  const nodes = sessionNodes(environment, session);
  const active = shownNode(nodes, session, picked);
  const group = environment.groups.find((known) => known.id === session.group);
  const root = group?.root ?? null;
  const key = `${runtime}/${name}`;

  function openTab(commands: ReturnType<typeof newChatCommands>, id: number) {
    if (environment.active_session !== session.id) sendCommand(store, runtime, selectSessionCommand(name, session.id));
    sendCommands(store, runtime, commands);
    pick(id);
  }

  function newChat() {
    const id = environment.next_id;
    openTab(newChatCommands(name, id, session.group, defaultChatHarness(settings), newResumeId), id);
  }

  function newTerminal(preset: AgentPreset) {
    const id = environment.next_id;
    openTab(newTerminalCommands(name, id, session.group, preset, harnessLabel(settings, preset), root), id);
  }

  function closeTab(id: number) {
    sendCommands(store, runtime, closeNodeCommands(name, id));
    if (picked === id) onPick(undefined);
  }

  function pick(id: number) {
    fileTabs.hide();
    onPick(id);
  }

  function body() {
    if (fileTabs.shown) {
      return <FileTabBody store={store} state={state} runtime={runtime} environment={name} root={root} path={fileTabs.shown} />;
    }
    if (!active) return <div className="empty">This session has no tabs.</div>;
    if (active.file) {
      return <FileTabBody store={store} state={state} runtime={runtime} environment={name} root={root} path={active.file.path} />;
    }
    const ref = { environment: name, terminal: active.id };
    if (active.kind === "Chat") {
      return <ChatView key={`${key}/${active.id}`} store={store} state={state} runtime={runtime} chat={ref} node={active} root={root} wide={wide} />;
    }
    return <TerminalView key={`${key}/${active.id}`} store={store} runtime={runtime} terminal={ref} wide={wide} />;
  }

  return (
    <>
      <TabStrip
        nodes={nodes}
        active={fileTabs.shown ? undefined : active?.id}
        fileTabs={fileTabs.tabs}
        shownFile={fileTabs.shown}
        filesShown={filesShown}
        filesHint={filesHint}
        wide={wide}
        onBack={onBack}
        onFiles={onFiles}
        onShow={pick}
        onAdd={add.show}
        onRename={(id, title) => sendCommand(store, runtime, renameNodeCommand(name, id, title))}
        onClose={closeTab}
        onShowFile={fileTabs.show}
        onPinFile={fileTabs.pin}
        onCloseFile={fileTabs.close}
        onPinNode={(id) => sendCommand(store, runtime, pinFileCommand(name, id))}
      />
      {add.open && <NewTabMenu settings={settings} wide={wide} onClose={add.close} onChat={newChat} onTerminal={newTerminal} />}
      {body()}
    </>
  );
}
