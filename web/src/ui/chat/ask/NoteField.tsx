import { useState, type KeyboardEvent } from "react";

export function NoteField({
  placeholder,
  wide,
  onSend,
  onCancel,
}: {
  placeholder: string;
  wide: boolean;
  onSend: (message: string) => void;
  onCancel: () => void;
}) {
  const [text, setText] = useState("");
  const keyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (wide && event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      onSend(text.trim());
    }
  };
  return (
    <div className="note">
      <textarea value={text} onChange={(event) => setText(event.target.value)} onKeyDown={keyDown} placeholder={placeholder} rows={2} />
      <div className="note-bar">
        <button className="button" onClick={onCancel}>
          Cancel
        </button>
        <button className="button primary" onClick={() => onSend(text.trim())}>
          Send
        </button>
      </div>
    </div>
  );
}
