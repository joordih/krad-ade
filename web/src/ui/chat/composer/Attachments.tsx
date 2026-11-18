import { useRef } from "react";
import { baseName } from "../../files/treeRows";
import { Icon } from "../../primitives/Icon";

const ATTACH_TOOLTIP = "Attach files, folders or images";
const REMOVE_TOOLTIP = "Remove";

export function AttachButton({ disabled, onFiles }: { disabled: boolean; onFiles: (files: File[]) => void }) {
  const picker = useRef<HTMLInputElement>(null);
  return (
    <>
      <button className="attach-button" title={ATTACH_TOOLTIP} aria-label={ATTACH_TOOLTIP} disabled={disabled} onClick={() => picker.current?.click()}>
        <Icon name="attachment-01" size="sm" tone="muted" />
      </button>
      <input
        ref={picker}
        className="hidden-input"
        type="file"
        multiple
        onChange={(event) => {
          onFiles(Array.from(event.target.files ?? []));
          event.target.value = "";
        }}
      />
    </>
  );
}

export function AttachedChips({ paths, onRemove }: { paths: readonly string[]; onRemove: (index: number) => void }) {
  if (paths.length === 0) return null;
  return (
    <div className="attached">
      {paths.map((path, index) => (
        <span key={path} className="attached-chip" title={path}>
          <Icon name="file-01" size="xs" tone="muted" />
          <span className="attached-name">{baseName(path)}</span>
          <button className="attached-remove" title={REMOVE_TOOLTIP} aria-label={REMOVE_TOOLTIP} onClick={() => onRemove(index)}>
            <Icon name="cancel-01" size="xs" tone="muted" />
          </button>
        </span>
      ))}
    </div>
  );
}
