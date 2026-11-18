import { newChatCommands, newSessionCommand, nextSessionId } from "../../state/commands/sessions";
import { environmentNamed } from "../../state/environments";
import { defaultChatHarness } from "../../state/harnesses";
import { newResumeId } from "../../state/ids";
import { kraddieGroups, kraddieSessionHidden } from "../../state/kraddies";
import { environmentNames, groupsOf, type SessionPlace } from "../../state/sessions";
import { LOCAL, type AppState, type AppStore } from "../../state/store";
import { Icon } from "../primitives/Icon";
import { sendCommand, sendCommands } from "../primitives/send";
import { SessionList } from "./SessionList";
import { SidebarHead } from "./SidebarHead";

const CLOUD_GROUP = "Cloud";

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
  selected: SessionPlace | undefined;
  onSelect: (selected: SessionPlace | undefined, terminal?: number) => void;
  onJump: () => void;
  attentionCount: number;
  wide: boolean;
}) {
  const groups = groupsOf(state, environment);
  const kraddies = kraddieGroups(state, environment);
  const list = { store, state, selected, wide, onSelect };

  function newSessionIn(group: number) {
    const local = environmentNamed(state.views[LOCAL]?.environments, environment);
    if (!local) return;
    const session = nextSessionId(local.sessions);
    const id = local.next_id;
    sendCommand(store, LOCAL, newSessionCommand(environment, session, group));
    sendCommands(store, LOCAL, newChatCommands(environment, id, group, defaultChatHarness(state.views[LOCAL]?.snapshot?.settings), newResumeId));
    onSelect({ runtime: LOCAL, environment, session }, id);
  }

  return (
    <nav className="sidebar">
      <SidebarHead
        environment={environment}
        environments={environmentNames(state)}
        onEnvironment={onEnvironment}
        attentionCount={attentionCount}
        onJump={onJump}
        wide={wide}
      />
      {kraddies.length > 0 && (
        <section className="kraddie-section">
          <div className="kraddie-head">
            <Icon name="kraddie" size="sm" />
            <span>Kraddies</span>
          </div>
          {kraddies.map((group) => (
            <div key={group.id} className="kraddie-group">
              <div className="kraddie-name">{group.name}</div>
              <SessionList {...list} items={group.sessions} indent />
            </div>
          ))}
        </section>
      )}
      {groups.map(({ group, sessions }) => (
        <section key={group?.id ?? CLOUD_GROUP}>
          <div className="group-head">
            <span className="group-name">{group?.name ?? CLOUD_GROUP}</span>
            {group?.worktree && (
              <span className="branch">
                <Icon name="git-branch" size="sm" />
                {group.worktree.branch}
              </span>
            )}
            {group && (
              <button
                className="round group-add"
                title="New session"
                aria-label={`New session in ${group.name}`}
                onClick={() => newSessionIn(group.id)}
              >
                <Icon name="add-01" size="sm" />
              </button>
            )}
          </div>
          <SessionList {...list} items={sessions.filter((item) => !kraddieSessionHidden(state, item))} />
        </section>
      ))}
      {groups.length === 0 && kraddies.length === 0 && <p className="sidebar-empty">No group in this environment yet.</p>}
    </nav>
  );
}
