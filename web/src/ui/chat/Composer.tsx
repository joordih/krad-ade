import { useState, type KeyboardEvent } from "react";
import type { AgentRef } from "../../protocol/AgentRef";
import type { ChatState } from "../../protocol/ChatState";
import type { Command } from "../../protocol/Command";
import type { ListedModels } from "../../protocol/ListedModels";
import { Icon } from "../Icon";
import {
  chosenModel,
  effortChoices,
  effortLabel,
  modelLabel,
  modelOffers,
  PERMISSION_MODES,
  permissionLabel,
} from "./composerOptions";
import { Picker, PickerItem } from "./Picker";

export function Composer({
  chat,
  state,
  send,
  wide,
  models,
}: {
  chat: AgentRef;
  state: ChatState | undefined;
  send: (command: Command) => void;
  wide: boolean;
  models: ListedModels[];
}) {
  const [text, setText] = useState("");
  const working = state?.working ?? false;
  const offers = modelOffers(models, state?.harness);
  const efforts = effortChoices(state, offers);
  const chosen = chosenModel(state);
  const queued = state?.queued ?? [];

  function submit() {
    const prompt = { text: text.trim(), mentions: [] };
    if (!prompt.text) return;
    send(working ? { area: "chat", verb: "queue", chat, prompt } : { area: "chat", verb: "send", chat, prompt });
    setText("");
  }

  function keyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (wide && event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      submit();
    }
  }

  return (
    <div className="composer">
      {queued.length > 0 && (
        <div className="queued">
          {queued.map((prompt, index) => (
            <div className="queued-row" key={`${index}/${prompt.text}`}>
              <span className="queued-text">{prompt.text}</span>
              <span className="queued-meta">
                Queued
                <button
                  className="link"
                  onClick={() => send({ area: "chat", verb: "unqueue", chat, index })}
                  title="Remove this queued prompt"
                >
                  Cancel
                </button>
              </span>
            </div>
          ))}
        </div>
      )}
      <textarea
        value={text}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={keyDown}
        placeholder={working ? "Queue a message…" : "Ask anything…"}
        rows={2}
      />
      <div className="bar">
        <div className="bar-group">
          <Picker
            icon="square-lock-02"
            label={permissionLabel(state?.permission_mode)}
            title="Permission mode"
            wide={wide}
          >
            {(dismiss) =>
              PERMISSION_MODES.map((mode) => (
                <PickerItem
                  key={mode}
                  label={permissionLabel(mode)}
                  checked={state?.permission_mode === mode}
                  onPick={() => {
                    send({ area: "chat", verb: "set_permission_mode", chat, mode });
                    dismiss();
                  }}
                />
              ))
            }
          </Picker>
        </div>
        <div className="bar-group end">
          <Picker label={modelLabel(state, offers)} title="Model" wide={wide}>
            {(dismiss) =>
              offers.length === 0 ? (
                <span className="menu-empty">No models</span>
              ) : (
                offers.map((offer) => (
                  <PickerItem
                    key={offer.value}
                    label={offer.name}
                    detail={offer.description ?? undefined}
                    checked={chosen === offer.value}
                    onPick={() => {
                      send({ area: "chat", verb: "set_model", chat, model: offer.value });
                      dismiss();
                    }}
                  />
                ))
              )
            }
          </Picker>
          <Picker icon="flash" label={effortLabel(state?.effort)} title="Reasoning effort" wide={wide}>
            {(dismiss) =>
              efforts.map((effort) => (
                <PickerItem
                  key={effort}
                  label={effortLabel(effort)}
                  checked={state?.effort === effort}
                  onPick={() => {
                    send({ area: "chat", verb: "set_effort", chat, effort });
                    dismiss();
                  }}
                />
              ))
            }
          </Picker>
          {working && (
            <button className="round" title="Interrupt" onClick={() => send({ area: "chat", verb: "interrupt", chat })}>
              <Icon name="stop-circle" small />
            </button>
          )}
          <button className="round primary" title={working ? "Queue" : "Send"} onClick={submit}>
            <Icon name="sent" small />
          </button>
        </div>
      </div>
    </div>
  );
}
