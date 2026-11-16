import { countIn } from "../state/attention";
import { kraddieGroups, kraddieSessionHidden } from "../state/kraddies";
import {
  chatNode,
  newSessionCommand,
  nodeTitle,
  openChatCommand,
  openNodeCommand,
  NEW_CHAT_LABEL,
  nextSessionId,
} from "../state/sessionEdits";
import { environmentNames, groupsOf } from "../state/sessions";
import { LOCAL, type AppState, type AppStore } from "../state/store";
import { forgetPairing } from "../runtime/http";
import { defaultChatHarness } from "./harnesses";
import { Icon } from "./Icon";
import { MenuItem, MenuSurface, MenuTrigger, useMenu } from "./Menu";
import { NotificationsButton } from "./NotificationsButton";
import { sendCommand } from "./send";
import { SessionRow } from "./SessionRow";
import { newResumeId } from "./uuid";

export type Selected = { runtime: string; environment: string; session: number } | undefined;

export function Sidebar({
  store,
  state,
  environment,
  onEnvironment,
  selected,
  onSelect,
  onJump,
  attentionCount,
  wide,
}: {
  store: AppStore;
  state: AppState;
  environment: string;
  onEnvironment: (name: string) => void;
  selected: Selected;
  onSelect: (selected: Selected, terminal?: number) => void;
  onJump: () => void;
  attentionCount: number;
  wide: boolean;
}) {
  const browser = useMenu();
  const groups = groupsOf(state, environment);
  const kraddies = kraddieGroups(state, environment);

  function isSelected(runtime: string, name: string, session: number): boolean {
    return selected?.runtime === runtime && selected.environment === name && selected.session === session;
  }

  function waitingIn(runtime: string, name: string, sessionNodes: readonly number[]): number {
    const view = state.views[runtime];
    return view ? countIn(view.attention.waiting, name, sessionNodes) : 0;
  }

  function newSessionIn(group: number) {
    const local = state.views[LOCAL]?.environments.find((known) => known.name === environment);
    if (!local) return;
    const session = nextSessionId(local.sessions);
    sendCommand(store, LOCAL, newSessionCommand(environment, session, group));
    const id = local.next_id;
    const harness = defaultChatHarness(state.views[LOCAL]?.snapshot?.settings);
    const node = chatNode(id, group, harness, nodeTitle(NEW_CHAT_LABEL, id));
    sendCommand(store, LOCAL, openNodeCommand(environment, node));
    sendCommand(store, LOCAL, openChatCommand({ environment, terminal: id }, node, newResumeId));
    onSelect({ runtime: LOCAL, environment, session }, id);
  }

  async function forget() {
    if (!window.confirm("Forget this browser? You will need a new pairing code to use it again.")) return;
    try {
      await forgetPairing();
      location.reload();
    } catch (error) {
      console.warn(error);
    }
  }

  return (
    <nav className="sidebar">
      <div className="sidebar-head">
        <label className="env-switch">
          <Icon name="layers-01" small />
          <select value={environment} onChange={(event) => onEnvironment(event.target.value)}>
            {environmentNames(state).map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
          <Icon name="arrow-down-01" small />
        </label>
        <div className="sidebar-tools">
          {attentionCount > 0 && (
            <button className="round jump" title="Jump to what needs attention" onClick={onJump}>
              <Icon name="target-02" small />
              <span className="attention-badge">{attentionCount}</span>
            </button>
          )}
          <NotificationsButton />
          <div className="head-menu">
            <MenuTrigger title="This browser" onClick={browser.show} className="head-more" />
            {browser.open && (
              <MenuSurface title="This browser" wide={wide} onClose={browser.close}>
                {(dismiss) => (
                  <MenuItem
                    icon="plug-socket"
                    label="Forget this browser"
                    danger
                    onPick={() => {
                      dismiss();
                      void forget();
                    }}
                  />
                )}
              </MenuSurface>
            )}
          </div>
        </div>
      </div>
      {kraddies.length > 0 && (
        <section className="kraddie-section">
          <div className="kraddie-head">
            <Icon name="kraddie" small />
            <span>Kraddies</span>
          </div>
          {kraddies.map((group) => (
            <div key={group.id} className="kraddie-group">
              <div className="kraddie-name">{group.name}</div>
              {group.sessions.map((item) => (
                <SessionRow
                  key={`${item.runtime}/${item.environment}/${item.session.id}`}
                  store={store}
                  item={item}
                  selected={isSelected(item.runtime, item.environment, item.session.id)}
                  waiting={waitingIn(item.runtime, item.environment, item.nodes.map((node) => node.id))}
                  wide={wide}
                  indent
                  onSelect={() => onSelect({ runtime: item.runtime, environment: item.environment, session: item.session.id })}
                  onClosed={() => {
                    if (isSelected(item.runtime, item.environment, item.session.id)) onSelect(undefined);
                  }}
                />
              ))}
            </div>
          ))}
        </section>
      )}
      {groups.map((group) => (
        <section key={group.group?.id ?? "cloud"}>
          <div className="group-head">
            <span>{group.group?.name ?? "Cloud"}</span>
            {group.group?.worktree && (
              <span className="branch">
                <Icon name="git-branch" small />
                {group.group.worktree.branch}
              </span>
            )}
            {group.group && (
              <button
                className="round group-add"
                title="New session"
                aria-label={`New session in ${group.group.name}`}
                onClick={() => newSessionIn(group.group!.id)}
              >
                <Icon name="add-01" small />
              </button>
            )}
          </div>
          {group.sessions
            .filter((item) => !kraddieSessionHidden(state, item))
            .map((item) => (
              <SessionRow
                key={`${item.runtime}/${item.environment}/${item.session.id}`}
                store={store}
                item={item}
                selected={isSelected(item.runtime, item.environment, item.session.id)}
                waiting={waitingIn(item.runtime, item.environment, item.nodes.map((node) => node.id))}
                wide={wide}
                onSelect={() => onSelect({ runtime: item.runtime, environment: item.environment, session: item.session.id })}
                onClosed={() => {
                  if (isSelected(item.runtime, item.environment, item.session.id)) onSelect(undefined);
                }}
              />
            ))}
        </section>
      ))}
      {groups.length === 0 && kraddies.length === 0 && (
        <p className="sidebar-empty">No group in this environment yet.</p>
      )}
    </nav>
  );
}
