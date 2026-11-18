import { useState } from "react";
import type { DirEntry } from "../../protocol/DirEntry";
import type { FileOpening } from "../../protocol/FileOpening";
import type { Host } from "../../protocol/Host";
import type { TerminalGroup } from "../../protocol/TerminalGroup";
import { listingOf } from "../../state/files";
import { changeMarks, groupGit, totalLines } from "../../state/git";
import type { AppState, AppStore, RuntimeKey } from "../../state/store";
import { Icon } from "../primitives/Icon";
import { warn } from "../primitives/send";
import { ChangesList } from "./ChangesList";
import { paletteFor } from "./fileIcons";
import { readSideSection, writeSideSection, type SideSection } from "./filesToggle";
import { FileTree } from "./FileTree";
import { toggled } from "./treeRows";
import { useFolderWatches } from "./useFolderWatches";
import { useSheetDark } from "./useSheetDark";

const PANEL_LABEL = "Files";
const CLOSE_LABEL = "Close";
const NO_FOLDER_TITLE = "No folder";
const NO_FOLDER_TEXT = "Select a group to browse its files.";
const ADDED_SIGN = "+";
const REMOVED_SIGN = "−";
const LOCAL_HOST: Host = { kind: "local" };

const SECTIONS: { section: SideSection; label: string; icon: string }[] = [
  { section: "explorer", label: "Explorer", icon: "file-01" },
  { section: "changes", label: "Changes", icon: "git-branch" },
];

export function FilesPanel({
  store,
  state,
  runtime,
  environment,
  host,
  group,
  connected,
  viewed,
  wide,
  onOpen,
  onClose,
}: {
  store: AppStore;
  state: AppState;
  runtime: RuntimeKey;
  environment: string;
  host: Host | undefined;
  group: TerminalGroup | undefined;
  connected: boolean;
  viewed: string | undefined;
  wide: boolean;
  onOpen: (path: string, opening: FileOpening) => void;
  onClose: () => void;
}) {
  const view = state.views[runtime];
  const palette = paletteFor(view?.snapshot?.settings.file_icon_colors, useSheetDark());
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set());
  const [section, setSection] = useState<SideSection>(() => readSideSection(localStorage));
  const root = group?.root ?? undefined;
  const refresh = useFolderWatches({ store, runtime, environment, root, expanded, connected });
  const git = root ? groupGit(view?.snapshot?.git ?? [], host ?? LOCAL_HOST, root) : undefined;
  const totals = git ? totalLines(git) : undefined;
  const changed = git?.status.changes.length ?? 0;

  function toggle(entry: DirEntry) {
    const opening = !expanded.has(entry.path);
    setExpanded((previous) => toggled(previous, entry.path));
    if (opening && !listingOf(view?.folders ?? {}, environment, entry.path)) {
      void store.loadFolder(runtime, environment, entry.path).catch(warn);
    }
  }

  function pickSection(next: SideSection) {
    writeSideSection(localStorage, next);
    setSection(next);
  }

  function body() {
    if (!root) {
      return (
        <div className="empty files-empty">
          <Icon name="folder-01" />
          <strong>{NO_FOLDER_TITLE}</strong>
          <span className="detail">{NO_FOLDER_TEXT}</span>
        </div>
      );
    }
    if (section === "changes") return <ChangesList git={git} palette={palette} viewed={viewed} onOpen={onOpen} />;
    return (
      <FileTree
        view={view}
        environment={environment}
        root={root}
        expanded={expanded}
        selected={viewed}
        palette={palette}
        marks={changeMarks(git)}
        onOpen={onOpen}
        onToggle={toggle}
        onCollapseAll={() => setExpanded(new Set())}
        onRefresh={refresh}
      />
    );
  }

  const panel = (
    <aside className={wide ? "files-panel" : "files-panel sheet"} aria-label={PANEL_LABEL}>
      <div className="side-sections">
        {SECTIONS.map((known) => (
          <button
            key={known.section}
            className={known.section === section ? "side-section active" : "side-section"}
            aria-pressed={known.section === section}
            onClick={() => pickSection(known.section)}
          >
            <Icon name={known.icon} size="sm" />
            {known.label}
            {known.section === "changes" && changed > 0 && <span className="side-section-count">{changed}</span>}
          </button>
        ))}
        {totals && (
          <span className="side-totals">
            <span className="side-added">{`${ADDED_SIGN}${totals.added}`}</span>
            <span className="side-removed">{`${REMOVED_SIGN}${totals.removed}`}</span>
          </span>
        )}
        {!wide && (
          <button className="round side-close" title={CLOSE_LABEL} aria-label={CLOSE_LABEL} onClick={onClose}>
            <Icon name="cancel-01" size="sm" />
          </button>
        )}
      </div>
      {body()}
    </aside>
  );
  if (wide) return panel;
  return (
    <>
      <button className="menu-scrim" title={CLOSE_LABEL} aria-label={CLOSE_LABEL} onClick={onClose} />
      {panel}
    </>
  );
}
