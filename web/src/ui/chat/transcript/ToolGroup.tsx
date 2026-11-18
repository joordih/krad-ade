import { useState } from "react";
import type { ToolCall } from "../../../protocol/ToolCall";
import { baseName } from "../../files/treeRows";
import { Icon } from "../../primitives/Icon";
import { doingLabel, doneSummary, lookOf, namesFile, oneLine, rowVerb, toolIcon } from "./tools";

function CallRow({ call }: { call: ToolCall }) {
  const look = lookOf(call);
  return (
    <div className="tool-call">
      <span className="tool-rail" aria-hidden="true" />
      <span className="tool-verb">{rowVerb(call)}</span>
      {namesFile(look) && call.summary ? (
        <span className="file-chip" title={call.summary}>
          <Icon name="file-01" size="xs" tone="muted" />
          {baseName(call.summary)}
        </span>
      ) : (
        <span className="tool-target">{oneLine(call.summary)}</span>
      )}
    </div>
  );
}

export function ToolGroup({ calls, running }: { calls: ToolCall[]; running: boolean }) {
  const [opened, setOpened] = useState<boolean>();
  const open = opened ?? running;
  const last = calls.at(-1);
  const look = last ? lookOf(last) : "other";
  return (
    <div className="tool-group">
      <button className={running ? "tool-head running" : "tool-head"} aria-expanded={open} onClick={() => setOpened(!open)}>
        <Icon name={toolIcon(look)} size="sm" tone="muted" />
        <span className="tool-label">{running ? doingLabel(look) : doneSummary(calls)}</span>
        <Icon name={open ? "arrow-up-01" : "arrow-down-01"} size="sm" tone="muted" />
      </button>
      {open && (
        <div className="tool-calls">
          {calls.map((call, index) => (
            <CallRow key={call.id ?? index} call={call} />
          ))}
        </div>
      )}
    </div>
  );
}
