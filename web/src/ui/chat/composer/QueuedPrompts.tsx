import type { Prompt } from "../../../protocol/Prompt";

export function QueuedPrompts({ queued, onCancel }: { queued: readonly Prompt[]; onCancel: (index: number) => void }) {
  if (queued.length === 0) return null;
  return (
    <div className="queued">
      {queued.map((prompt, index) => (
        <div className="queued-row" key={`${index}/${prompt.text}`}>
          <span className="queued-text">{prompt.text}</span>
          <span className="queued-meta">
            Queued
            <button className="link" onClick={() => onCancel(index)} title="Remove this queued prompt">
              Cancel
            </button>
          </span>
        </div>
      ))}
    </div>
  );
}
