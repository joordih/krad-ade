import { useRef, useState, type KeyboardEvent } from "react";
import type { AgentRef } from "../../../protocol/AgentRef";
import type { ChatState } from "../../../protocol/ChatState";
import type { Command } from "../../../protocol/Command";
import type { Host } from "../../../protocol/Host";
import type { Session } from "../../../protocol/Session";
import { upload } from "../../../runtime/http";
import type { AppStore, RuntimeKey } from "../../../state/store";
import {
  interruptCommand,
  pickModelCommands,
  promptCommand,
  setEffortCommand,
  setPermissionModeCommand,
  unqueueCommand,
} from "../../../state/commands/chat";
import { hasRemote } from "../../../state/git";
import { harnessIcon, harnessTone } from "../../../state/harnesses";
import { newResumeId } from "../../../state/ids";
import type { RuntimeView } from "../../../state/runtimeView";
import { warn } from "../../primitives/send";
import { Icon } from "../../primitives/Icon";
import { AttachButton, AttachedChips } from "./Attachments";
import { chatModelFacts } from "./chatModels";
import { sendState } from "./controls";
import { EffortControl } from "./EffortControl";
import { ModeControl } from "./ModeControl";
import { ModelControl } from "./ModelControl";
import { toggledPreferences, type ModelChoice, type ModelMark } from "./models";
import { QueuedPrompts } from "./QueuedPrompts";
import { HERE, hostLabel, runOnChoices, runOnOffered, type RunOn } from "./runOn";
import { RunOnControl } from "./RunOnControl";
import { SendButton } from "./SendButton";
import { SuggestionMenu } from "./SuggestionMenu";
import { accepted, mentionText } from "./suggestions";
import { useSuggestions, type Suggestion } from "./useSuggestions";
import { UsageRing } from "./UsageRing";
import { useMedia } from "../../shell/useWide";

const PLACEHOLDER = "Ask anything…  @ for files, / for skills";
const DESCRIBED_QUERY = "(min-width: 1100px)";

export type ComposerPlace = {
  view: RuntimeView | undefined;
  host: Host | undefined;
  root: string | null;
  session: Session | undefined;
  fresh: boolean;
  switchable: boolean;
  inCloud: boolean;
  canDrive: boolean;
  canAdmin: boolean;
};

export function Composer({
  store,
  runtime,
  chat,
  state,
  place,
  wide,
  send,
}: {
  store: AppStore;
  runtime: RuntimeKey;
  chat: AgentRef;
  state: ChatState | undefined;
  place: ComposerPlace;
  wide: boolean;
  send: (command: Command) => void;
}) {
  const [text, setText] = useState("");
  const [cursor, setCursor] = useState(0);
  const [attached, setAttached] = useState<string[]>([]);
  const [runOn, setRunOn] = useState<RunOn>(HERE);
  const field = useRef<HTMLTextAreaElement>(null);
  const described = useMedia(DESCRIBED_QUERY);
  const snapshot = place.view?.snapshot;
  const settings = snapshot?.settings;
  const harness = state?.harness;
  const working = state?.working ?? false;
  const facts = chatModelFacts(state, snapshot, {
    environment: chat.environment,
    host: place.host,
    switchable: place.switchable,
    inCloud: place.inCloud,
  });
  const suggestions = useSuggestions({
    store,
    runtime,
    environment: chat.environment,
    root: place.root,
    skills: state?.commands ?? [],
    text,
    cursor,
  });
  const runners = settings?.cloud?.runners ?? [];
  const offered = runOnOffered({
    runners: runners.length,
    fresh: place.fresh,
    cloudSession: place.session?.cloud != null,
    remote: place.root !== null && place.host !== undefined && hasRemote(snapshot?.git ?? [], place.host, place.root),
  });
  const choices = offered ? runOnChoices(hostLabel(place.host), runners, snapshot?.runners.shown ?? []) : [];
  const empty = text.trim().length === 0 && attached.length === 0;

  function edited(next: string, at: number) {
    setText(next);
    setCursor(at);
  }

  function submit() {
    const mentions = attached.map((path) => ({ Path: path }));
    const command =
      offered && runOn.kind === "runner" && place.session
        ? sentToRunner(runOn.runner, mentions)
        : promptCommand(chat, text, mentions, working);
    if (!command) return;
    send(command);
    edited("", 0);
    setAttached([]);
  }

  function sentToRunner(runner: number, mentions: { Path: string }[]): Command | undefined {
    if (!place.session || (!text.trim() && mentions.length === 0)) return undefined;
    return {
      area: "cloud",
      verb: "send",
      environment: chat.environment,
      session: place.session.id,
      runner,
      prompt: { chat: chat.terminal, prompt: { text: text.trim(), mentions } },
    };
  }

  function pickSuggestion(row: Suggestion) {
    const open = suggestions.suggested;
    if (!open) return;
    const value = row.kind === "skill" ? row.skill.name : mentionText(row.entry.path, row.entry.kind === "directory");
    const next = accepted(text, open.trigger, value);
    edited(next.text, next.cursor);
    requestAnimationFrame(() => field.current?.setSelectionRange(next.cursor, next.cursor));
  }

  function keyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    const open = suggestions.suggested;
    if (open) {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        suggestions.move(event.key === "ArrowDown");
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        suggestions.dismiss();
        return;
      }
      const row = open.rows?.[open.selected];
      if ((event.key === "Enter" || event.key === "Tab") && row) {
        event.preventDefault();
        pickSuggestion(row);
        return;
      }
    }
    if (wide && event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      submit();
    }
  }

  async function attach(files: File[]) {
    const root = place.root;
    if (root === null) return;
    for (const file of files) {
      try {
        const path = await upload(chat.environment, root, file);
        setAttached((known) => (known.includes(path) ? known : [...known, path]));
      } catch (error) {
        warn(error);
      }
    }
  }

  function mark(choice: ModelChoice, which: ModelMark) {
    if (!settings) return;
    send({
      area: "settings",
      verb: "save",
      settings: { ...settings, model_preferences: toggledPreferences(settings.model_preferences, choice.harness, choice.id, which) },
    });
  }

  function pickModel(choice: ModelChoice) {
    if (!harness) return;
    for (const command of pickModelCommands(chat, harness, choice.harness, choice.id, newResumeId)) send(command);
  }

  return (
    <div className="composer">
      <AttachedChips paths={attached} onRemove={(index) => setAttached((known) => known.filter((_, at) => at !== index))} />
      <QueuedPrompts queued={state?.queued ?? []} onCancel={(index) => send(unqueueCommand(chat, index))} />
      <div className="composer-field">
        <textarea
          ref={field}
          className="composer-input"
          value={text}
          rows={1}
          placeholder={PLACEHOLDER}
          onChange={(event) => edited(event.target.value, event.target.selectionStart)}
          onSelect={(event) => setCursor(event.currentTarget.selectionStart)}
          onKeyDown={keyDown}
          onBlur={suggestions.dismiss}
        />
        <SendButton state={sendState(empty, working)} onSend={submit} onStop={() => send(interruptCommand(chat))} />
        {suggestions.suggested && <SuggestionMenu suggested={suggestions.suggested} onPick={pickSuggestion} onHover={suggestions.select} />}
      </div>
      <div className="composer-settings">
        <AttachButton disabled={!place.canDrive || place.root === null} onFiles={(files) => void attach(files)} />
        <ModeControl mode={state?.permission_mode} wide={wide} onMode={(mode) => send(setPermissionModeCommand(chat, mode))} />
        {offered && <RunOnControl choices={choices} chosen={runOn} wide={wide} onChoose={setRunOn} />}
        <div className="model-controls">
          {harness && <Icon name={harnessIcon(harness)} size="xs" tone={harnessTone(harness)} />}
          {facts.account && <span className="composer-account">{facts.account}</span>}
          {harness && (
            <ModelControl
              facts={facts}
              harness={harness}
              settings={settings}
              wide={wide}
              described={described}
              canMark={place.canAdmin}
              onModel={pickModel}
              onMark={mark}
            />
          )}
          <EffortControl facts={facts} wide={wide} onEffort={(effort) => send(setEffortCommand(chat, effort))} />
          <UsageRing percent={facts.contextPercent} window={facts.contextWindow} />
        </div>
      </div>
    </div>
  );
}
