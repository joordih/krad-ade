import type { FileOpening } from "../../protocol/FileOpening";
import type { GroupGit } from "../../protocol/GroupGit";
import { changedPath, changeFolder, changeMark, changeSections, changeTone } from "../../state/git";
import { Icon } from "../primitives/Icon";
import type { IconPaletteId } from "./fileIcons";
import { FileIcon } from "./FileIcon";
import { baseName } from "./treeRows";

const NO_REPO_TITLE = "No repository";
const NO_REPO_TEXT = "The selected group's folder is not a git repository.";
const CLEAN_TITLE = "No changes";
const CLEAN_TEXT = "The working tree matches the last commit.";
const ADDED_SIGN = "+";
const REMOVED_SIGN = "−";

export function ChangesList({
  git,
  palette,
  viewed,
  onOpen,
}: {
  git: GroupGit | undefined;
  palette: IconPaletteId;
  viewed: string | undefined;
  onOpen: (path: string, opening: FileOpening) => void;
}) {
  if (!git) {
    return (
      <div className="empty files-empty">
        <Icon name="git-branch" />
        <strong>{NO_REPO_TITLE}</strong>
        <span className="detail">{NO_REPO_TEXT}</span>
      </div>
    );
  }
  if (git.status.changes.length === 0) {
    return (
      <div className="empty files-empty">
        <Icon name="checkmark-circle-02" />
        <strong>{CLEAN_TITLE}</strong>
        <span className="detail">{CLEAN_TEXT}</span>
      </div>
    );
  }
  return (
    <div className="files-tree-scroll changes-list">
      {changeSections(git).map((section) => (
        <div key={section.label} className="changes-section">
          <div className="changes-section-head">
            <Icon name="arrow-down-01" size="sm" tone="subtle" />
            <span className="changes-section-label">{section.label}</span>
            <span className="changes-section-count">{section.changes.length}</span>
          </div>
          {section.changes.map((change) => {
            const path = changedPath(git, change);
            const lines = git.lines[change.path];
            const name = baseName(change.path);
            const folder = changeFolder(change.path);
            const tone = changeTone(change.kind);
            return (
              <button
                key={`${section.label}/${change.path}`}
                className={path === viewed ? "files-row change-row active" : "files-row change-row"}
                title={change.path}
                onClick={() => onOpen(path, "preview")}
                onDoubleClick={() => onOpen(path, "pinned")}
              >
                <FileIcon name={name} palette={palette} />
                <span className={`change-name tone-${tone}`}>{name}</span>
                <span className="change-folder">{folder}</span>
                {lines && lines.added > 0 && <span className="change-count tone-added">{`${ADDED_SIGN}${lines.added}`}</span>}
                {lines && lines.removed > 0 && <span className="change-count tone-removed">{`${REMOVED_SIGN}${lines.removed}`}</span>}
                <span className={`change-mark tone-${tone}`}>{changeMark(change.kind)}</span>
              </button>
            );
          })}
        </div>
      ))}
    </div>
  );
}
