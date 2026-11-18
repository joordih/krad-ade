import { useState, type KeyboardEvent } from "react";
import type { AgentPreset } from "../../../protocol/AgentPreset";
import { harnessIcon, harnessLabel, harnessTone, type HarnessSource } from "../../../state/harnesses";
import { Icon } from "../../primitives/Icon";
import { Floating } from "../../primitives/Menu";
import { contextLabel, modelRows, openingHarness, railHarnesses, railStep, steppedHarness, type ModelChoice, type ModelMark } from "./models";

const MODEL_TITLE = "Model";
const SEARCH_PLACEHOLDER = "Search models";
const NO_MODELS = "No models";
const RECOMMENDED_BADGE = "Recommended";
const DEFAULT_BADGE = "Default";
const MISSING_BADGE = "Unavailable";
const ADD_FAVORITE = "Add to favorites";
const REMOVE_FAVORITE = "Remove from favorites";
const SET_DEFAULT = "Use for new chats of this agent";
const CLEAR_DEFAULT = "Stop using for new chats";

function Badges({ choice }: { choice: ModelChoice }) {
  return (
    <>
      {choice.isDefault && <span className="model-badge strong">{DEFAULT_BADGE}</span>}
      {choice.recommended && <span className="model-badge">{RECOMMENDED_BADGE}</span>}
      {choice.context && <span className="model-badge">{contextLabel(choice.context)}</span>}
      {choice.missing && <span className="model-badge warning">{MISSING_BADGE}</span>}
    </>
  );
}

function MarkButton({ icon, on, title, onClick }: { icon: string; on: boolean; title: string; onClick: () => void }) {
  return (
    <span
      role="button"
      tabIndex={-1}
      className={on ? "model-mark on" : "model-mark"}
      title={title}
      aria-label={title}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
    >
      <Icon name={icon} size="sm" tone={on ? "accent" : "subtle"} />
    </span>
  );
}

function ModelRowButton({
  choice,
  highlighted,
  described,
  canMark,
  onHover,
  onPick,
  onMark,
}: {
  choice: ModelChoice;
  highlighted: boolean;
  described: boolean;
  canMark: boolean;
  onHover: () => void;
  onPick: () => void;
  onMark: (mark: ModelMark) => void;
}) {
  const tall = described && choice.description;
  const marks = canMark && !choice.missing;
  const classes = ["model-row"];
  if (highlighted) classes.push("highlighted");
  if (tall) classes.push("tall");
  return (
    <button className={classes.join(" ")} role="menuitemradio" aria-checked={choice.current} onMouseEnter={onHover} onFocus={onHover} onClick={onPick}>
      <span className="model-text">
        <span className="model-name-line">
          <span className="model-name">{choice.label}</span>
          <Badges choice={choice} />
        </span>
        {tall && <span className="model-description">{choice.description}</span>}
      </span>
      {marks && (highlighted || choice.isDefault) && (
        <MarkButton icon="target-02" on={choice.isDefault} title={choice.isDefault ? CLEAR_DEFAULT : SET_DEFAULT} onClick={() => onMark("default")} />
      )}
      {marks && (highlighted || choice.favorite) && (
        <MarkButton
          icon={choice.favorite ? "star-filled" : "star"}
          on={choice.favorite}
          title={choice.favorite ? REMOVE_FAVORITE : ADD_FAVORITE}
          onClick={() => onMark("favorite")}
        />
      )}
      {choice.current && <Icon name="tick-02" size="sm" tone="strong" />}
    </button>
  );
}

function RailButton({ harness, settings, selected, onShow }: { harness: AgentPreset; settings: HarnessSource | undefined; selected: boolean; onShow: () => void }) {
  const label = harnessLabel(settings, harness);
  return (
    <button className={selected ? "model-rail-button selected" : "model-rail-button"} title={label} aria-label={label} aria-pressed={selected} onClick={onShow}>
      <Icon name={harnessIcon(harness)} tone={harnessTone(harness)} />
    </button>
  );
}

export function ModelMenu({
  harness,
  settings,
  choices,
  wide,
  described,
  canMark,
  onPick,
  onMark,
  onClose,
}: {
  harness: AgentPreset;
  settings: HarnessSource | undefined;
  choices: readonly ModelChoice[];
  wide: boolean;
  described: boolean;
  canMark: boolean;
  onPick: (choice: ModelChoice) => void;
  onMark: (choice: ModelChoice, mark: ModelMark) => void;
  onClose: () => void;
}) {
  const rail = railHarnesses(choices);
  const [shown, setShown] = useState<AgentPreset>(() => openingHarness(choices) ?? harness);
  const [query, setQuery] = useState("");
  const rows = modelRows(choices.filter((choice) => choice.harness === shown), query);
  const models = rows.flatMap((row) => (row.kind === "model" ? [row.choice] : []));
  const [highlighted, setHighlighted] = useState(() => Math.max(models.findIndex((choice) => choice.current), 0));
  const at = Math.min(highlighted, Math.max(models.length - 1, 0));

  function show(next: AgentPreset) {
    setShown(next);
    setQuery("");
    const opened = choices.filter((choice) => choice.harness === next);
    setHighlighted(Math.max(opened.findIndex((choice) => choice.current), 0));
  }

  function keyDown(event: KeyboardEvent<HTMLInputElement>) {
    const count = models.length;
    if (event.key === "Escape") return onClose();
    const step = railStep(event.key, event.shiftKey);
    if (step !== undefined && rail.length > 1 && (event.key === "Tab" || query === "")) {
      event.preventDefault();
      show(steppedHarness(rail, shown, step));
      return;
    }
    if (count === 0) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      setHighlighted((at + (event.key === "ArrowDown" ? 1 : count - 1)) % count);
    } else if (event.key === "Enter") {
      event.preventDefault();
      const picked = models[at];
      if (picked) onPick(picked);
    }
  }

  let index = -1;
  return (
    <Floating title={MODEL_TITLE} wide={wide} onClose={onClose} placement="above-end" className="model-menu" bare>
      <div className="model-menu-body">
        <div className="model-rail">
          {(rail.length > 0 ? rail : [harness]).map((known) => (
            <RailButton key={known} harness={known} settings={settings} selected={known === shown} onShow={() => show(known)} />
          ))}
        </div>
        <div className="model-main">
          <label className="model-search">
            <Icon name="search-01" tone="muted" />
            <input
              value={query}
              autoFocus={wide}
              placeholder={SEARCH_PLACEHOLDER}
              aria-label={SEARCH_PLACEHOLDER}
              onChange={(event) => {
                setQuery(event.target.value);
                setHighlighted(0);
              }}
              onKeyDown={keyDown}
            />
          </label>
          <div className="model-list" role="menu" aria-label={MODEL_TITLE}>
            {rows.map((row) => {
              if (row.kind === "section") return <div key={`section/${row.label}`} className="model-section">{row.label}</div>;
              index += 1;
              const mine = index;
              return (
                <ModelRowButton
                  key={`${row.choice.harness}/${row.choice.id}`}
                  choice={row.choice}
                  highlighted={mine === at}
                  described={described}
                  canMark={canMark}
                  onHover={() => setHighlighted(mine)}
                  onPick={() => onPick(row.choice)}
                  onMark={(mark) => onMark(row.choice, mark)}
                />
              );
            })}
            {models.length === 0 && <div className="menu-empty">{NO_MODELS}</div>}
          </div>
        </div>
      </div>
    </Floating>
  );
}
