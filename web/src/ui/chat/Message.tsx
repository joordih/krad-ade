import { useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { AnswerPart } from "../../protocol/AnswerPart";
import type { ChatMessage } from "../../protocol/ChatMessage";
import { Icon } from "../Icon";

const TOOL_ICONS: Record<string, string> = {
  Shell: "command-line",
  Read: "file-01",
  Edit: "pencil-edit-01",
  Search: "search-01",
  Fetch: "square-arrow-up-right",
  Subagent: "user-group",
  Checklist: "task-done-01",
  Question: "bubble-chat",
  Plan: "list-view",
  Other: "flash",
};

function Thinking({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  return (
    <div>
      <button className="thinking" onClick={() => setOpen(!open)}>
        <Icon name="idea-01" small />
        Thinking
        <Icon name={open ? "arrow-up-01" : "arrow-down-01"} small />
      </button>
      {open && <div className="answer" style={{ color: "var(--krad-text-muted)", whiteSpace: "pre-wrap" }}>{text}</div>}
    </div>
  );
}

function Part({ part }: { part: AnswerPart }) {
  if ("text" in part) {
    return (
      <div className="answer">
        <Markdown remarkPlugins={[remarkGfm]}>{part.text}</Markdown>
      </div>
    );
  }
  if ("thinking" in part) return <Thinking text={part.thinking} />;
  if ("tool" in part) {
    return (
      <div className="tool-row">
        <Icon name={TOOL_ICONS[part.tool.kind] ?? "flash"} small />
        <span>{part.tool.name}</span>
        <span className="summary">{part.tool.summary}</span>
      </div>
    );
  }
  return (
    <div className="tool-row">
      <Icon name="checkmark-circle-02" small />
      <span className="summary">{part.ask.subject}</span>
    </div>
  );
}

export function Message({ message }: { message: ChatMessage }) {
  if (message.speaker === "user") {
    return <div className="user-bubble">{message.text}</div>;
  }
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--krad-space-2)" }}>
      {message.parts.map((part, index) => (
        <Part key={index} part={part} />
      ))}
      {message.error && <div className="error-text">{message.error}</div>}
      {message.running && message.parts.length === 0 && (
        <div className="tool-row">
          <Icon name="loading-03" small />
          Working…
        </div>
      )}
    </div>
  );
}
