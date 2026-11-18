import { useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { ChatMessage } from "../../../protocol/ChatMessage";
import { Icon } from "../../primitives/Icon";
import { ToolGroup } from "./ToolGroup";
import { blocksOf, type Block } from "./tools";

const REMARK_PLUGINS = [remarkGfm];

function Thinking({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="thought">
      <button className="thinking" aria-expanded={open} onClick={() => setOpen(!open)}>
        <Icon name="idea-01" size="sm" />
        Thinking
        <Icon name={open ? "arrow-up-01" : "arrow-down-01"} size="sm" />
      </button>
      {open && <div className="thought-text">{text}</div>}
    </div>
  );
}

function BlockView({ block, running }: { block: Block; running: boolean }) {
  if (block.kind === "tools") return <ToolGroup calls={block.calls} running={running} />;
  const part = block.part;
  if ("text" in part) {
    return (
      <div className="answer">
        <Markdown remarkPlugins={REMARK_PLUGINS}>{part.text}</Markdown>
      </div>
    );
  }
  if ("thinking" in part) return <Thinking text={part.thinking} />;
  return (
    <div className="tool-head settled">
      <Icon name="checkmark-circle-02" size="sm" tone="muted" />
      <span className="tool-label">{part.ask.subject}</span>
    </div>
  );
}

export function Message({ message }: { message: ChatMessage }) {
  if (message.speaker === "user") return <div className="user-bubble">{message.text}</div>;
  const blocks = blocksOf(message.parts);
  return (
    <div className="assistant">
      {blocks.map((block, index) => (
        <BlockView key={block.at} block={block} running={message.running && index === blocks.length - 1} />
      ))}
      {message.error && <div className="error-text">{message.error}</div>}
      {message.running && message.parts.length === 0 && (
        <div className="tool-head running">
          <Icon name="loading-03" size="sm" tone="muted" />
          <span className="tool-label">Working…</span>
        </div>
      )}
    </div>
  );
}
