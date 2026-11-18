import { Icon } from "../../primitives/Icon";
import { entryLabels } from "./suggestions";
import type { Suggested, Suggestion } from "./useSuggestions";

const LOADING = "Loading…";
const NO_FILES = "No matching files or folders";
const NO_SKILLS = "No matching skills";

function labelsOf(row: Suggestion): { icon: string; name: string; detail: string } {
  if (row.kind === "skill") return { icon: "ai-magic", name: row.skill.name, detail: row.skill.description };
  const { name, folder } = entryLabels(row.entry.path);
  return { icon: row.entry.kind === "directory" ? "folder-01" : "file-01", name, detail: folder };
}

export function SuggestionMenu({ suggested, onPick, onHover }: { suggested: Suggested; onPick: (row: Suggestion) => void; onHover: (index: number) => void }) {
  const { rows, trigger, selected } = suggested;
  const empty = trigger.kind === "mention" ? NO_FILES : NO_SKILLS;
  return (
    <div className="suggestions" role="listbox">
      {rows === undefined && <div className="suggestion-note">{LOADING}</div>}
      {rows?.length === 0 && <div className="suggestion-note">{empty}</div>}
      {rows?.map((row, index) => {
        const { icon, name, detail } = labelsOf(row);
        return (
          <button
            key={`${name}/${detail}`}
            className={index === selected ? "suggestion selected" : "suggestion"}
            role="option"
            aria-selected={index === selected}
            onMouseDown={(event) => event.preventDefault()}
            onMouseEnter={() => onHover(index)}
            onClick={() => onPick(row)}
          >
            <Icon name={icon} size="sm" tone="muted" />
            <span className="suggestion-name">{name}</span>
            <span className="suggestion-detail">{detail}</span>
          </button>
        );
      })}
    </div>
  );
}
