import { closeSessionCommands, renameSessionCommand, toggleAttentionCommand } from "../../state/commands/sessions";
import type { SessionItem } from "../../state/sessions";
import type { AppStore } from "../../state/store";
import { Icon } from "../primitives/Icon";
import { useLongPress } from "../primitives/longPress";
import { MenuItem, MenuSeparator, MenuSurface, MenuTrigger, useMenu } from "../primitives/Menu";
import { RenameField, useRename } from "../primitives/RenameField";
import { sendCommand, sendCommands } from "../primitives/send";
import type { HarnessSource } from "../../state/harnesses";
import { NEEDS_YOU, sessionCaption, sessionIcon, statusLook } from "./sessionDetail";

const CLOSE_QUESTION = "Close this session? Its terminals and chats close with it.";
const CLOSE_TOOLTIP = "Close session; its terminals and chats close with it";
const MUTE_TOOLTIP = "Mute notifications and sounds from this session";
const UNMUTE_TOOLTIP = "Notify again about this session";

export function SessionRow({
  store,
  item,
  selected,
  waiting,
  settings,
  wide,
  indent = false,
  onSelect,
  onClosed,
}: {
  store: AppStore;
  item: SessionItem;
  selected: boolean;
  waiting: number;
  settings: HarnessSource | undefined;
  wide: boolean;
  indent?: boolean;
  onSelect: () => void;
  onClosed: () => void;
}) {
  const menu = useMenu();
  const longPress = useLongPress(menu.show);
  const { session, environment, runtime } = item;
  const muted = session.attention === "muted";
  const rename = useRename((draft) => {
    if (draft.trim() !== session.name) sendCommand(store, runtime, renameSessionCommand(environment, session.id, draft));
  });

  function close() {
    if (!window.confirm(CLOSE_QUESTION)) return;
    sendCommands(store, runtime, closeSessionCommands(environment, session.id, item.nodes.map((node) => node.id)));
    onClosed();
  }

  const needsYou = item.state === "WaitingForInput" || waiting > 0;
  const status = statusLook(item.state);
  const toggleMute = () => sendCommand(store, runtime, toggleAttentionCommand(environment, session.id));
  const classes = ["session-row"];
  if (selected) classes.push("selected");
  if (needsYou) classes.push("unread");
  const rowClass = classes.join(" ");
  const glyph = <Icon name={sessionIcon(session.look?.icon)} size="sm" tone="muted" />;

  return (
    <div className={indent ? "session-row-wrap indented" : "session-row-wrap"}>
      {rename.renaming ? (
        <div className={`${rowClass} renaming`}>
          {glyph}
          <RenameField rename={rename} label="Session name" className="rename-input" />
        </div>
      ) : (
        <div
          role="button"
          tabIndex={0}
          className={rowClass}
          onClick={() => {
            if (!longPress.consume()) onSelect();
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") onSelect();
          }}
          onContextMenu={(event) => {
            event.preventDefault();
            menu.show();
          }}
          {...longPress.handlers}
        >
          {glyph}
          <span className="texts">
            <span className="name">{session.name}</span>
            <span className="detail">{sessionCaption(item.nodes, settings)}</span>
          </span>
          {item.cloud && <Icon name="cloud" size="sm" tone="muted" />}
          {wide && (
            <span className="hover-tools">
              <button
                className="row-tool"
                title={muted ? UNMUTE_TOOLTIP : MUTE_TOOLTIP}
                aria-label={muted ? UNMUTE_TOOLTIP : MUTE_TOOLTIP}
                onClick={(event) => {
                  event.stopPropagation();
                  toggleMute();
                }}
              >
                <Icon name={muted ? "notification-01" : "notification-off-01"} size="sm" />
              </button>
              {selected && (
                <button
                  className="row-tool"
                  title={CLOSE_TOOLTIP}
                  aria-label={CLOSE_TOOLTIP}
                  onClick={(event) => {
                    event.stopPropagation();
                    close();
                  }}
                >
                  <Icon name="cancel-01" size="sm" />
                </button>
              )}
            </span>
          )}
          {muted && <Icon name="notification-off-01" size="sm" tone="muted" />}
          {needsYou && (
            <span className="needs-you" title={waiting > 0 ? `${waiting} waiting for you` : undefined}>
              {NEEDS_YOU}
            </span>
          )}
          {status && <Icon name={status.icon} size="sm" tone={status.tone} spins={status.spins} />}
        </div>
      )}
      {!wide && <MenuTrigger title={`${session.name} actions`} onClick={menu.show} />}
      {menu.open && (
        <MenuSurface title={session.name} wide={wide} onClose={menu.close} className="fill">
          {(dismiss) => (
            <>
              <MenuItem
                icon="pencil-edit-01"
                label="Rename"
                onPick={() => {
                  rename.start(session.name);
                  dismiss();
                }}
              />
              <MenuItem
                icon={muted ? "notification-01" : "notification-off-01"}
                label={muted ? "Notify again about this session" : "Mute this session"}
                onPick={() => {
                  toggleMute();
                  dismiss();
                }}
              />
              <MenuSeparator />
              <MenuItem
                icon="cancel-01"
                label="Close session"
                danger
                onPick={() => {
                  dismiss();
                  close();
                }}
              />
            </>
          )}
        </MenuSurface>
      )}
    </div>
  );
}
