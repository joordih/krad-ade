import { useEffect, useRef } from "react";
import type { AgentRef } from "../../protocol/AgentRef";
import type { Command } from "../../protocol/Command";
import type { TerminalNode } from "../../protocol/TerminalNode";
import { openChatCommand } from "../../state/sessionEdits";
import { chatKey } from "../../state/runtimeView";
import type { AppStore, AppState, RuntimeKey } from "../../state/store";
import { Icon } from "../Icon";
import { sendCommand } from "../send";
import { newResumeId } from "../uuid";
import { AskCard } from "./AskCard";
import { Composer } from "./Composer";
import { Message } from "./Message";

const FOLLOW_SLACK_PX = 80;

export function ChatView({
  store,
  state,
  runtime,
  chat,
  node,
  wide,
}: {
  store: AppStore;
  state: AppState;
  runtime: RuntimeKey;
  chat: AgentRef;
  node?: TerminalNode;
  wide: boolean;
}) {
  const view = state.views[runtime];
  const shown = view?.chats[chatKey(chat)];
  const models = view?.snapshot?.models.listed ?? [];
  const scroller = useRef<HTMLDivElement>(null);
  const following = useRef(true);
  const count = shown?.messages.length ?? 0;
  const lastText = shown?.messages.at(-1)?.text.length ?? 0;

  useEffect(() => {
    const element = scroller.current;
    if (element && following.current) element.scrollTop = element.scrollHeight;
  }, [count, lastText]);

  const send = (command: Command) => {
    void store.send(runtime, command).catch((error: unknown) => console.warn(error));
  };

  if (!shown) {
    const resume = node?.resume;
    return (
      <div className="empty chat-start">
        <Icon name="bubble-chat" />
        <div className="chat-start-text">
          <strong>This chat has not started on its runtime yet.</strong>
          <span className="detail">
            {resume
              ? "Open it here to resume where the agent left off."
              : "Open it here to start the agent in this tab."}
          </span>
        </div>
        <button
          className="button primary"
          onClick={() => sendCommand(store, runtime, openChatCommand(chat, node, newResumeId))}
        >
          {resume ? "Resume chat" : "Open chat"}
        </button>
      </div>
    );
  }
  return (
    <div className="chat">
      <div
        className="transcript"
        ref={scroller}
        onScroll={(event) => {
          const element = event.currentTarget;
          following.current = element.scrollHeight - element.scrollTop - element.clientHeight < FOLLOW_SLACK_PX;
        }}
      >
        <div className="transcript-inner">
          {shown.from > 0 && (
            <button className="earlier" onClick={() => void store.loadEarlier(runtime, chat, shown.from)}>
              Show earlier messages
            </button>
          )}
          {shown.messages.map((message, index) => (
            <Message key={shown.from + index} message={message} />
          ))}
          {shown.state?.problem && <div className="error-text">{shown.state.problem}</div>}
        </div>
      </div>
      {shown.asks.map((ask) => (
        <AskCard key={ask.ask} ask={ask} send={send} wide={wide} />
      ))}
      <Composer chat={chat} state={shown.state} send={send} wide={wide} models={models} />
    </div>
  );
}
