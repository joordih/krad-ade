import { useEffect, useRef } from "react";
import type { AgentRef } from "../../protocol/AgentRef";
import type { Command } from "../../protocol/Command";
import type { TerminalNode } from "../../protocol/TerminalNode";
import { openChatCommand } from "../../state/commands/sessions";
import { environmentNamed } from "../../state/environments";
import { newResumeId } from "../../state/ids";
import { harnessSwitchable } from "../../state/chats";
import { chatKey } from "../../state/runtimeView";
import { sessionOfTerminal } from "../../state/sessions";
import type { AppState, AppStore, RuntimeKey } from "../../state/store";
import { sendCommand } from "../primitives/send";
import { AskCard } from "./ask/AskCard";
import { ChatStart } from "./ChatStart";
import { Composer } from "./composer/Composer";
import { Message } from "./transcript/Message";

const FOLLOW_SLACK_PX = 80;
const CHAT_QUESTION = "What should we work on in krad?";
const EARLIER_LABEL = "Show earlier messages";

export function ChatView({
  store,
  state,
  runtime,
  chat,
  node,
  root,
  wide,
}: {
  store: AppStore;
  state: AppState;
  runtime: RuntimeKey;
  chat: AgentRef;
  node: TerminalNode | undefined;
  root: string | null;
  wide: boolean;
}) {
  const view = state.views[runtime];
  const shown = view?.chats[chatKey(chat)];
  const scroller = useRef<HTMLDivElement>(null);
  const following = useRef(true);
  const count = shown?.messages.length ?? 0;
  const lastText = shown?.messages.at(-1)?.text.length ?? 0;

  useEffect(() => {
    const element = scroller.current;
    if (element && following.current) element.scrollTop = element.scrollHeight;
  }, [count, lastText]);

  const send = (command: Command) => sendCommand(store, runtime, command);

  if (!shown) return <ChatStart resume={Boolean(node?.resume)} onOpen={() => send(openChatCommand(chat, node, newResumeId))} />;

  const environment = environmentNamed(view?.environments, chat.environment);
  const permissions = store.client(runtime)?.permissions ?? [];
  const landing = shown.messages.length === 0 && shown.from === 0;
  return (
    <div className={landing ? "chat landing" : "chat"}>
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
              {EARLIER_LABEL}
            </button>
          )}
          {shown.messages.map((message, index) => (
            <Message key={shown.from + index} message={message} />
          ))}
          {shown.state?.problem && <div className="error-text">{shown.state.problem}</div>}
        </div>
      </div>
      <div className="chat-foot">
        {landing && <div className="chat-question">{CHAT_QUESTION}</div>}
        {shown.asks.map((ask) => (
          <AskCard key={ask.ask} ask={ask} send={send} wide={wide} />
        ))}
        <Composer
          store={store}
          runtime={runtime}
          chat={chat}
          state={shown.state}
          wide={wide}
          send={send}
          place={{
            view,
            host: environment?.host,
            root,
            session: environment ? sessionOfTerminal(environment, chat.terminal) : undefined,
            fresh: landing,
            switchable: harnessSwitchable(runtime, shown),
            inCloud: node?.cloud != null,
            canDrive: permissions.includes("drive"),
            canAdmin: permissions.includes("admin"),
          }}
        />
      </div>
      {landing && <div className="chat-landing-rest" />}
    </div>
  );
}
