import { useState } from "react";
import type { CodeDiagnostic } from "../../protocol/CodeDiagnostic";
import type { AppStore, RuntimeKey } from "../../state/store";
import { languageFor, languageLabel } from "../highlight/languages";
import { Icon } from "../primitives/Icon";
import { CodeLines } from "./CodeLines";
import { FileIcon } from "./FileIcon";
import type { IconPaletteId } from "./fileIcons";
import { ProblemList } from "./ProblemList";
import { severityByLine, sortedDiagnostics } from "./problems";
import { baseName, relativePath } from "./treeRows";
import { useFileRead, useHighlight } from "./useFileRead";

const LINE_BREAK = "\n";

export function FileViewer({
  store,
  runtime,
  environment,
  root,
  path,
  diagnostics,
  palette,
  connected,
}: {
  store: AppStore;
  runtime: RuntimeKey;
  environment: string;
  root: string;
  path: string;
  diagnostics: CodeDiagnostic[];
  palette: IconPaletteId;
  connected: boolean;
}) {
  const [reload, setReload] = useState(0);
  const loaded = useFileRead(store, runtime, environment, path, `${reload}/${connected}`);
  const name = baseName(path);
  const read = loaded.kind === "ready" ? loaded.read : undefined;
  const text = read?.kind === "text" ? read.text : undefined;
  const tokens = useHighlight(text, languageFor(name));

  return (
    <div className="files-viewer">
      <div className="files-viewer-head">
        <FileIcon name={name} palette={palette} />
        <div className="files-viewer-title">
          <span className="files-viewer-name">{name}</span>
          <span className="files-viewer-path">{relativePath(root, path)}</span>
        </div>
        <span className="files-language">{languageLabel(name)}</span>
        <button className="round files-viewer-action" title="Reload this file" aria-label="Reload this file" onClick={() => setReload((count) => count + 1)}>
          <Icon name="refresh" size="sm" />
        </button>
      </div>
      {read === undefined ? (
        <div className="empty">Opening this file…</div>
      ) : read.kind === "error" ? (
        <div className="empty files-empty">
          <strong>Can't open this file</strong>
          <span className="detail">{read.message}</span>
        </div>
      ) : (
        <CodeLines lines={read.text.split(LINE_BREAK)} tokens={tokens} markers={severityByLine(diagnostics)} />
      )}
      {read?.kind === "text" && read.truncated && <div className="files-note">This file is too large to show in full; only its start is shown.</div>}
      <ProblemList problems={sortedDiagnostics(diagnostics)} />
    </div>
  );
}
