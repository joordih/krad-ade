import { useState } from "react";
import { sessionNodes } from "../state/sessions";
import type { AppStore, AppState } from "../state/store";
import { ChatView } from "./chat/ChatView";
import { Icon } from "./Icon";
import { TerminalView } from "./terminal/TerminalView";
import type { Selected } from "./Sidebar";

export function SessionView({
  store,
  state,
  selected,
  onBack,
  wide,
}: {
  store: AppStore;
  state: AppState;
  selected: Selected;
  onBack: () => void;
  wide: boolean;
}) {
  const [picked, setPicked] = useState<number>();
  const environment = selected && state.views[selected.runtime]?.environments.find((known) => known.name === selected.environment);
  const session = environment?.sessions?.find((known) => known.id === selected?.session);
  if (!selected || !environment || !session) {
    return <div className="empty">Pick a session.</div>;
  }
  const nodes = sessionNodes(environment, session);
  const fallback = nodes.find((node) => node.id === session.last_chat) ?? nodes.find((node) => node.kind === "Chat") ?? nodes[0];
  const active = nodes.find((node) => node.id === picked) ?? fallback;
  return (
    <>
      <div className="tabs">
        <button className="round back" onClick={onBack} title="Sessions">
          <Icon name="arrow-left-01" small />
        </button>
        {nodes.map((node) => (
          <button
            key={node.id}
            className={node.id === active?.id ? "tab active" : "tab"}
            onClick={() => setPicked(node.id)}
          >
            <span className={`dot ${node.state}`} />
            <Icon name={node.kind === "Chat" ? "bubble-chat" : "computer-terminal-01"} small />
            {node.title}
          </button>
        ))}
      </div>
      {active?.kind === "Chat" ? (
        <ChatView
          key={`${selected.runtime}/${environment.name}/${active.id}`}
          store={store}
          state={state}
          runtime={selected.runtime}
          chat={{ environment: environment.name, terminal: active.id }}
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
