import type { CSSProperties } from "react";
import type { DirEntry } from "../../protocol/DirEntry";
import type { FileOpening } from "../../protocol/FileOpening";
import { changeMark, changeTone, type ChangeMarks } from "../../state/git";
import type { RuntimeView } from "../../state/runtimeView";
import { diagnosticsOf, listingOf } from "../../state/files";
import { Icon } from "../primitives/Icon";
import { FileIcon } from "./FileIcon";
import type { IconPaletteId } from "./fileIcons";
import { problemSeverity } from "./problems";
import { folderLabel, treeRows } from "./treeRows";

const RELOAD_LABEL = "Reload the files from disk";
const COLLAPSE_LABEL = "Collapse all folders";

export function FileTree({
  view,
  environment,
  root,
  expanded,
  selected,
  palette,
  marks,
  onOpen,
  onToggle,
  onCollapseAll,
  onRefresh,
}: {
  view: RuntimeView | undefined;
  environment: string;
  root: string;
  expanded: ReadonlySet<string>;
  selected: string | undefined;
  palette: IconPaletteId;
  marks: ChangeMarks;
  onOpen: (path: string, opening: FileOpening) => void;
  onToggle: (entry: DirEntry) => void;
  onCollapseAll: () => void;
  onRefresh: () => void;
}) {
  const rows = treeRows(
    root,
    (dir) => (view ? listingOf(view.folders, environment, dir) : undefined),
    expanded,
  );
  const listing = view ? listingOf(view.folders, environment, root) : undefined;
  return (
    <div className="files-tree">
      <div className="files-root">
        <span className="files-root-folder" title={root}>
          <Icon name="folder-01" size="sm" tone="muted" />
          <span className="files-root-name">{folderLabel(root)}</span>
          <Icon name="arrow-down-01" size="xs" tone="muted" />
        </span>
        <span className="files-tree-actions">
          <button className="round files-tree-action" title={RELOAD_LABEL} aria-label={RELOAD_LABEL} onClick={onRefresh}>
            <Icon name="refresh" size="sm" />
          </button>
          <button className="round files-tree-action" title={COLLAPSE_LABEL} aria-label={COLLAPSE_LABEL} onClick={onCollapseAll}>
            <Icon name="collapse-all" size="sm" />
          </button>
        </span>
      </div>
      <div className="files-tree-scroll">
        {rows.map((row) => {
          const directory = row.entry.kind === "directory";
          const problem = view
            ? problemSeverity(diagnosticsOf(view.diagnostics, environment, row.entry.path))
            : undefined;
          const change = marks.files.get(row.entry.path);
          const folderChanged = directory && marks.folders.has(row.entry.path);
          const classes = ["files-row"];
          if (row.entry.path === selected) classes.push("active");
          if (problem) classes.push(`problem-${problem}`);
          if (change) classes.push(`tone-${changeTone(change)}`);
          return (
            <button
              key={row.entry.path}
              className={classes.join(" ")}
              title={row.entry.path}
              style={{ "--krad-web-depth": row.depth } as CSSProperties}
              onClick={() => (directory ? onToggle(row.entry) : onOpen(row.entry.path, "preview"))}
              onDoubleClick={() => {
                if (!directory) onOpen(row.entry.path, "pinned");
              }}
            >
              <span className="files-chevron">
                {directory && <Icon name={row.expanded ? "arrow-down-01" : "arrow-right-01"} size="sm" />}
              </span>
              <FileIcon
                name={row.entry.name}
                folder={directory}
                expanded={row.expanded}
                palette={palette}
              />
              <span className="files-name">{row.entry.name}</span>
              {change && <span className={`change-mark tone-${changeTone(change)}`}>{changeMark(change)}</span>}
              {folderChanged && <span className="files-folder-dot" />}
            </button>
          );
        })}
        {listing !== undefined && listing.length === 0 && (
          <p className="sidebar-empty">This folder is empty.</p>
        )}
        {listing === undefined && <p className="sidebar-empty">Opening this folder…</p>}
      </div>
    </div>
  );
}
