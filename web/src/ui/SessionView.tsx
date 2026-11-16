import { useRef, useState } from "react";
import type { AgentPreset } from "../protocol/AgentPreset";
import type { TerminalNode } from "../protocol/TerminalNode";
import {
  chatNode,
  closeNodeCommands,
  NEW_CHAT_LABEL,
  nodeTitle,
  openChatCommand,
  openNodeCommand,
  openTerminalCommand,
  renameNodeCommand,
  selectSessionCommand,
  terminalNode,
} from "../state/sessionEdits";
import { sessionNodes, shownNode } from "../state/sessions";
import type { AppStore, AppState } from "../state/store";
import { ChatView } from "./chat/ChatView";
import {
  chatHarnessChoices,
  defaultChatHarness,
  harnessChoices,
  harnessLabel,
} from "./harnesses";
import { Icon } from "./Icon";
import { useLongPress } from "./longPress";
import { MenuItem, MenuSection, MenuSeparator, MenuSurface, MenuTrigger, useMenu } from "./Menu";
import { sendCommand } from "./send";
import type { Selected } from "./Sidebar";
import { TerminalView } from "./terminal/TerminalView";
import { newResumeId } from "./uuid";

function iconOf(node: TerminalNode): string {
  if (node.kind === "Chat") return "bubble-chat";
  if (node.kind === "File") return "file-01";
  if (node.kind === "Extension") return "workflow-square-10";
  return "computer-terminal-01";
}

function TabButton({
  node,
  active,
  onShow,
  onMenu,
}: {
  node: TerminalNode;
  active: boolean;
  onShow: () => void;
  onMenu: () => void;
}) {
  const longPress = useLongPress(onMenu);
  return (
    <div className="tab-wrap">
      <button
        className={active ? "tab active" : "tab"}
        onClick={() => {
          if (longPress.consume()) return;
          onShow();
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          onMenu();
        }}
        onPointerDown={longPress.onPointerDown}
        onPointerMove={longPress.onPointerMove}
        onPointerUp={longPress.onPointerUp}
        onPointerCancel={longPress.onPointerCancel}
      >
        <span className={`dot ${node.state}`} />
        <Icon name={iconOf(node)} small />
        <span className="tab-title">{node.title}</span>
      </button>
      <MenuTrigger title={`${node.title} actions`} onClick={onMenu} className="tab-more" />
    </div>
  );
}

export function SessionView({
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
  selected: Selected;
  picked: number | undefined;
  onPick: (terminal: number | undefined) => void;
  onBack: () => void;
  wide: boolean;
}) {
  const add = useMenu();
  const [menuTab, setMenuTab] = useState<TerminalNode>();
  const [renaming, setRenaming] = useState<number>();
  const [draft, setDraft] = useState("");
  const cancelled = useRef(false);
  const found = selected && state.views[selected.runtime]?.environments.find((known) => known.name === selected.environment);
  const foundSession = found?.sessions?.find((known) => known.id === selected?.session);
  if (!selected || !found || !foundSession) {
    return <div className="empty">{selected ? "Opening this session…" : "Pick a session."}</div>;
  }
  const environment = found;
  const session = foundSession;
  const runtime = selected.runtime;
  const settings = state.views[runtime]?.snapshot?.settings;
  const nodes = sessionNodes(environment, session);
  const active = shownNode(nodes, session, picked);
  const group = session.group;
  const groupRoot = environment.groups.find((known) => known.id === group)?.root ?? null;
  const defaultHarness = defaultChatHarness(settings);
  const chats = chatHarnessChoices(settings);
  const harnesses = harnessChoices(settings);

  function selectIfNeeded() {
    if (environment.active_session !== session.id) {
      sendCommand(store, runtime, selectSessionCommand(environment.name, session.id));
    }
  }

  function newChat(harness: AgentPreset) {
    const id = environment.next_id;
    const node = chatNode(id, group, harness, nodeTitle(NEW_CHAT_LABEL, id));
    selectIfNeeded();
    sendCommand(store, runtime, openNodeCommand(environment.name, node));
    sendCommand(
      store,
      runtime,
      openChatCommand({ environment: environment.name, terminal: id }, node, newResumeId, harness),
    );
    onPick(id);
  }

  function newTerminal(preset: AgentPreset) {
    const id = environment.next_id;
    const label = harnessLabel(settings, preset);
    const node = terminalNode(id, group, nodeTitle(label, id));
    selectIfNeeded();
    sendCommand(store, runtime, openNodeCommand(environment.name, node));
    sendCommand(store, runtime, openTerminalCommand(environment.name, id, preset, groupRoot));
    onPick(id);
  }

  function closeTab(id: number) {
    for (const command of closeNodeCommands(environment.name, id)) sendCommand(store, runtime, command);
    if (picked === id) onPick(undefined);
  }

  function commitRename(id: number) {
    if (cancelled.current) {
      cancelled.current = false;
      return;
    }
    setRenaming(undefined);
    const command = renameNodeCommand(environment.name, id, draft);
    if (command) sendCommand(store, runtime, command);
  }

  return (
    <>
      <div className="tabs">
        <button className="round back" onClick={onBack} title="Sessions">
          <Icon name="arrow-left-01" small />
        </button>
        {nodes.map((node) =>
          renaming === node.id ? (
            <div key={node.id} className="tab renaming">
              <input
                className="tab-rename-input"
                value={draft}
                autoFocus
                aria-label="Tab name"
                onChange={(event) => setDraft(event.target.value)}
                onBlur={() => commitRename(node.id)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.currentTarget.blur();
                  } else if (event.key === "Escape") {
                    cancelled.current = true;
                    setRenaming(undefined);
                  }
                }}
              />
            </div>
          ) : (
            <TabButton
              key={node.id}
              node={node}
              active={node.id === active?.id}
              onShow={() => onPick(node.id)}
              onMenu={() => setMenuTab(node)}
            />
          ),
        )}
        <MenuTrigger title="New tab" onClick={add.show} className="tab-add" icon="add-01" />
      </div>
      {add.open && (
        <MenuSurface title="New tab" wide={wide} onClose={add.close} className="under-tabs">
          {(dismiss) => (
            <>
              <MenuItem
                icon="bubble-chat"
                label="New chat"
                detail={harnessLabel(settings, defaultHarness)}
                onPick={() => {
                  dismiss();
                  newChat(defaultHarness);
                }}
              />
              <MenuSeparator />
              <MenuSection>Chat with</MenuSection>
              {chats.map((choice) => (
                <MenuItem
                  key={choice.preset}
                  icon={choice.icon}
                  label={choice.label}
                  onPick={() => {
                    dismiss();
                    newChat(choice.preset);
                  }}
                />
              ))}
              <MenuSeparator />
              <MenuSection>Terminal with</MenuSection>
              {harnesses.map((choice) => (
                <MenuItem
                  key={choice.preset}
                  icon={choice.icon}
                  label={choice.label}
                  onPick={() => {
                    dismiss();
                    newTerminal(choice.preset);
                  }}
                />
              ))}
            </>
          )}
        </MenuSurface>
      )}
      {menuTab && (
        <MenuSurface
          title={menuTab.title}
          wide={wide}
          onClose={() => setMenuTab(undefined)}
          className="under-tabs"
        >
          {(dismiss) => (
            <>
              <MenuItem
                icon="pencil-edit-01"
                label="Rename"
                onPick={() => {
                  setDraft(menuTab.title);
                  cancelled.current = false;
                  setRenaming(menuTab.id);
                  dismiss();
                }}
              />
              <MenuItem
                icon="cancel-01"
                label="Close tab"
                danger
                onPick={() => {
                  dismiss();
                  closeTab(menuTab.id);
                }}
              />
            </>
          )}
        </MenuSurface>
      )}
      {active?.kind === "Chat" ? (
        <ChatView
          key={`${selected.runtime}/${environment.name}/${active.id}`}
          store={store}
          state={state}
          runtime={selected.runtime}
          chat={{ environment: environment.name, terminal: active.id }}
          node={active}
          wide={wide}
        />
      ) : active ? (
        <TerminalView
          key={`${selected.runtime}/${environment.name}/${active.id}`}
          store={store}
          runtime={selected.runtime}
          terminal={{ environment: environment.name, terminal: active.id }}
          wide={wide}
        />
      ) : (
        <div className="empty">This session has no tabs.</div>
      )}
    </>
  );
}
