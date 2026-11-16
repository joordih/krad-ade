import { useRef, useState } from "react";
import type { Command } from "../protocol/Command";
import {
  closeSessionCommands,
  renameSessionCommand,
  toggleAttentionCommand,
} from "../state/sessionEdits";
import type { SessionItem } from "../state/sessions";
import type { AppStore } from "../state/store";
import { Icon } from "./Icon";
import { useLongPress } from "./longPress";
import { MenuItem, MenuSeparator, MenuSurface, MenuTrigger, useMenu } from "./Menu";
import { sendCommand } from "./send";

const STATE_LABEL: Record<string, string> = {
  Working: "Working",
  WaitingForInput: "Waiting for you",
  Done: "Done",
  Idle: "Idle",
};

const CLOSE_QUESTION = "Close this session? Its terminals and chats close with it.";

function detail(item: SessionItem): string {
  const chats = item.nodes.filter((node) => node.kind === "Chat").length;
  const terminals = item.nodes.length - chats;
  const parts = [STATE_LABEL[item.state] ?? item.state];
  if (chats) parts.push(`${chats} ${chats === 1 ? "chat" : "chats"}`);
  if (terminals) parts.push(`${terminals} ${terminals === 1 ? "terminal" : "terminals"}`);
  if (item.cloud) parts.push("cloud");
  return parts.join(" · ");
}

/** One session row: state, attention badge and the actions its menu holds. */
export function SessionRow({
  store,
  item,
  selected,
  waiting,
  wide,
  indent = false,
  onSelect,
  onClosed,
}: {
  store: AppStore;
  item: SessionItem;
  selected: boolean;
  waiting: number;
  wide: boolean;
  indent?: boolean;
  onSelect: () => void;
  onClosed: () => void;
}) {
  const menu = useMenu();
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState(item.session.name);
  const cancelled = useRef(false);
  const muted = item.session.attention === "muted";
  const longPress = useLongPress(menu.show);

  const send = (command: Command | undefined) => {
    if (command) sendCommand(store, item.runtime, command);
  };

  function commitRename() {
    if (cancelled.current) {
      cancelled.current = false;
      return;
    }
    setRenaming(false);
    if (draft.trim() && draft.trim() !== item.session.name) {
      send(renameSessionCommand(item.environment, item.session.id, draft));
    }
  }

  function close() {
    if (!window.confirm(CLOSE_QUESTION)) return;
    const commands = closeSessionCommands(
      item.environment,
      item.session.id,
      item.nodes.map((node) => node.id),
    );
    for (const command of commands) sendCommand(store, item.runtime, command);
    onClosed();
  }

  const rowClass = selected ? "session-row selected" : "session-row";

  return (
    <div className={indent ? "session-row-wrap indented" : "session-row-wrap"}>
      {renaming ? (
        <div className={`${rowClass} renaming`}>
          <span className={`dot ${item.state}`} />
          <input
            className="rename-input"
            value={draft}
            autoFocus
            aria-label="Session name"
            onChange={(event) => setDraft(event.target.value)}
            onBlur={commitRename}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.currentTarget.blur();
              } else if (event.key === "Escape") {
                cancelled.current = true;
                setRenaming(false);
              }
            }}
          />
        </div>
      ) : (
        <button
          className={rowClass}
          onClick={() => {
            if (longPress.consume()) return;
            onSelect();
          }}
          onContextMenu={(event) => {
            event.preventDefault();
            menu.show();
          }}
          onPointerDown={longPress.onPointerDown}
          onPointerMove={longPress.onPointerMove}
          onPointerUp={longPress.onPointerUp}
          onPointerCancel={longPress.onPointerCancel}
        >
          <span className={`dot ${item.state}`} />
          <span className="texts">
            <span className="name">{item.session.name}</span>
            <span className="detail">{detail(item)}</span>
          </span>
          {item.cloud && <Icon name="cloud" small />}
          {muted && <Icon name="notification-off-01" small />}
          {waiting > 0 && (
            <span className="attention-badge" title={`${waiting} waiting for you`}>
              {waiting}
            </span>
          )}
        </button>
      )}
      <MenuTrigger title={`${item.session.name} actions`} onClick={menu.show} />
      {menu.open && (
        <MenuSurface title={item.session.name} wide={wide} onClose={menu.close}>
          {(dismiss) => (
            <>
              <MenuItem
                icon="pencil-edit-01"
                label="Rename"
                onPick={() => {
                  setDraft(item.session.name);
                  cancelled.current = false;
                  setRenaming(true);
                  dismiss();
                }}
              />
              <MenuItem
                icon={muted ? "notification-01" : "notification-off-01"}
                label={muted ? "Notify again about this session" : "Mute this session"}
                onPick={() => {
                  send(toggleAttentionCommand(item.environment, item.session.id));
                  dismiss();
                }}
              />
              <MenuSeparator />
              <MenuItem icon="cancel-01" label="Close session" danger onPick={() => {
                dismiss();
                close();
              }} />
            </>
          )}
        </MenuSurface>
      )}
    </div>
  );
}
