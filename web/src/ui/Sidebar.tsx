import { environmentNames, groupsOf, type SessionItem } from "../state/sessions";
import type { AppState } from "../state/store";
import { Icon } from "./Icon";

export type Selected = { runtime: string; environment: string; session: number } | undefined;

const STATE_LABEL: Record<string, string> = {
  Working: "Working",
  WaitingForInput: "Waiting for you",
  Done: "Done",
  Idle: "Idle",
};

function detail(item: SessionItem): string {
  const chats = item.nodes.filter((node) => node.kind === "Chat").length;
  const terminals = item.nodes.length - chats;
  const parts = [STATE_LABEL[item.state] ?? item.state];
  if (chats) parts.push(`${chats} ${chats === 1 ? "chat" : "chats"}`);
  if (terminals) parts.push(`${terminals} ${terminals === 1 ? "terminal" : "terminals"}`);
  if (item.cloud) parts.push("cloud");
  return parts.join(" · ");
}

export function Sidebar({
  state,
  environment,
  onEnvironment,
  selected,
  onSelect,
}: {
  state: AppState;
  environment: string;
  onEnvironment: (name: string) => void;
  selected: Selected;
  onSelect: (selected: Selected) => void;
}) {
  const groups = groupsOf(state, environment);
  return (
    <nav className="sidebar">
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
          </div>
          {group.sessions.map((item) => {
            const isSelected =
              selected?.runtime === item.runtime &&
              selected.environment === item.environment &&
              selected.session === item.session.id;
            return (
              <button
                key={`${item.runtime}/${item.environment}/${item.session.id}`}
                className={isSelected ? "session-row selected" : "session-row"}
                onClick={() => onSelect({ runtime: item.runtime, environment: item.environment, session: item.session.id })}
              >
                <span className={`dot ${item.state}`} />
                <span className="texts">
                  <span className="name">{item.session.name}</span>
                  <span className="detail">{detail(item)}</span>
                </span>
                {item.cloud && <Icon name="cloud" small />}
              </button>
            );
          })}
        </section>
      ))}
    </nav>
  );
}
