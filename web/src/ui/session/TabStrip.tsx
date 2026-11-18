import { useState } from "react";
import type { TerminalNode } from "../../protocol/TerminalNode";
import type { FileTab } from "../../state/fileTabs";
import { baseName } from "../files/treeRows";
import { Icon } from "../primitives/Icon";
import { useLongPress } from "../primitives/longPress";
import { MenuItem, MenuSurface, MenuTrigger } from "../primitives/Menu";
import { RenameField, useRename } from "../primitives/RenameField";
import { badgeLook, stateDot, tabMarks } from "./tabLook";

const CLOSE_TAB = "Close tab";
const ADD_TAB = "New tab";
const FILES_LABEL = "Files";
const PIN_TAB = "Pin Tab";
const FILE_ICON = "file-01";

type MenuTab = { kind: "node"; node: TerminalNode } | { kind: "file"; tab: FileTab };

function FileTabButton({
  tab,
  active,
  wide,
  onShow,
  onPin,
  onMenu,
  onClose,
}: {
  tab: FileTab;
  active: boolean;
  wide: boolean;
  onShow: () => void;
  onPin: () => void;
  onMenu: () => void;
  onClose: () => void;
}) {
  const longPress = useLongPress(onMenu);
  const name = baseName(tab.path);
  return (
    <div className={active ? "tab-wrap active" : "tab-wrap"}>
      <button
        className={active ? "tab active" : "tab"}
        title={tab.path}
        onClick={() => {
          if (!longPress.consume()) onShow();
        }}
        onDoubleClick={() => {
          if (!tab.pinned) onPin();
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          onMenu();
        }}
        {...longPress.handlers}
      >
        <span className="tab-badge bare">
          <Icon name={FILE_ICON} size="sm" />
        </span>
        <span className={tab.pinned ? "tab-title" : "tab-title preview"}>{name}</span>
      </button>
      {wide ? (
        <button className="tab-close" title={CLOSE_TAB} aria-label={CLOSE_TAB} onClick={onClose}>
          <Icon name="cancel-01" size="xs" />
        </button>
      ) : (
        <MenuTrigger title={`${name} actions`} onClick={onMenu} className="tab-more" />
      )}
    </div>
  );
}

function TabButton({
  node,
  active,
  wide,
  onShow,
  onPin,
  onMenu,
  onClose,
}: {
  node: TerminalNode;
  active: boolean;
  wide: boolean;
  onShow: () => void;
  onPin: () => void;
  onMenu: () => void;
  onClose: () => void;
}) {
  const longPress = useLongPress(onMenu);
  const badge = badgeLook(node);
  const preview = node.file ? !node.file.pinned : false;
  const dot = stateDot(node.state);
  return (
    <div className={active ? "tab-wrap active" : "tab-wrap"}>
      <button
        className={active ? "tab active" : "tab"}
        onClick={() => {
          if (!longPress.consume()) onShow();
        }}
        onDoubleClick={() => {
          if (preview) onPin();
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          onMenu();
        }}
        {...longPress.handlers}
      >
        <span className={`tab-badge ${badge.fill}`}>
          <Icon name={badge.icon} size="xs" />
          {dot && <span className={`tab-dot ${dot}`} />}
        </span>
        <span className={preview ? "tab-title preview" : "tab-title"}>{node.title}</span>
        {tabMarks(node).map((mark) => (
          <Icon key={mark} name={mark} size="xs" tone="muted" />
        ))}
      </button>
      {wide ? (
        <button className="tab-close" title={CLOSE_TAB} aria-label={CLOSE_TAB} onClick={onClose}>
          <Icon name="cancel-01" size="xs" />
        </button>
      ) : (
        <MenuTrigger title={`${node.title} actions`} onClick={onMenu} className="tab-more" />
      )}
    </div>
  );
}

export function TabStrip({
  nodes,
  active,
  fileTabs,
  shownFile,
  filesShown,
  filesHint,
  wide,
  onBack,
  onFiles,
  onShow,
  onAdd,
  onRename,
  onClose,
  onShowFile,
  onPinFile,
  onCloseFile,
  onPinNode,
}: {
  nodes: readonly TerminalNode[];
  active: number | undefined;
  fileTabs: readonly FileTab[];
  shownFile: string | undefined;
  filesShown: boolean;
  filesHint: string;
  wide: boolean;
  onBack: () => void;
  onFiles: () => void;
  onShow: (id: number) => void;
  onAdd: () => void;
  onRename: (id: number, title: string) => void;
  onClose: (id: number) => void;
  onShowFile: (path: string) => void;
  onPinFile: (path: string) => void;
  onCloseFile: (path: string) => void;
  onPinNode: (id: number) => void;
}) {
  const [menuTab, setMenuTab] = useState<MenuTab>();
  const [renamed, setRenamed] = useState<number>();
  const rename = useRename((draft) => {
    if (renamed !== undefined) onRename(renamed, draft);
  });

  return (
    <>
      <div className="tabs">
        <button className="round back" onClick={onBack} title="Sessions">
          <Icon name="arrow-left-01" size="sm" />
        </button>
        {nodes.map((node) =>
          rename.renaming && renamed === node.id ? (
            <div key={node.id} className="tab renaming">
              <RenameField rename={rename} label="Tab name" className="tab-rename-input" />
            </div>
          ) : (
            <TabButton
              key={node.id}
              node={node}
              active={node.id === active}
              wide={wide}
              onShow={() => onShow(node.id)}
              onPin={() => onPinNode(node.id)}
              onMenu={() => setMenuTab({ kind: "node", node })}
              onClose={() => onClose(node.id)}
            />
          ),
        )}
        {fileTabs.map((tab) => (
          <FileTabButton
            key={tab.path}
            tab={tab}
            active={tab.path === shownFile}
            wide={wide}
            onShow={() => onShowFile(tab.path)}
            onPin={() => onPinFile(tab.path)}
            onMenu={() => setMenuTab({ kind: "file", tab })}
            onClose={() => onCloseFile(tab.path)}
          />
        ))}
        <MenuTrigger title={ADD_TAB} onClick={onAdd} className="tab-add" icon="add-01" />
        <button
          className={filesShown ? "round files-toggle on" : "round files-toggle"}
          title={wide ? `${FILES_LABEL} (${filesHint})` : FILES_LABEL}
          aria-label={FILES_LABEL}
          aria-pressed={filesShown}
          onClick={onFiles}
        >
          <Icon name="sidebar-right" size="sm" />
        </button>
      </div>
      {menuTab && (
        <MenuSurface title={menuTitle(menuTab)} wide={wide} onClose={() => setMenuTab(undefined)} placement="under-tabs">
          {(dismiss) => (
            <>
              {menuPreview(menuTab) && (
                <MenuItem
                  icon="tick-02"
                  label={PIN_TAB}
                  onPick={() => {
                    dismiss();
                    if (menuTab.kind === "file") onPinFile(menuTab.tab.path);
                    else onPinNode(menuTab.node.id);
                  }}
                />
              )}
              {menuTab.kind === "node" && (
                <MenuItem
                  icon="pencil-edit-01"
                  label="Rename"
                  onPick={() => {
                    setRenamed(menuTab.node.id);
                    rename.start(menuTab.node.title);
                    dismiss();
                  }}
                />
              )}
              <MenuItem
                icon="cancel-01"
                label={CLOSE_TAB}
                danger
                onPick={() => {
                  dismiss();
                  if (menuTab.kind === "file") onCloseFile(menuTab.tab.path);
                  else onClose(menuTab.node.id);
                }}
              />
            </>
          )}
        </MenuSurface>
      )}
    </>
  );
}

function menuTitle(menu: MenuTab): string {
  return menu.kind === "file" ? baseName(menu.tab.path) : menu.node.title;
}

function menuPreview(menu: MenuTab): boolean {
  if (menu.kind === "file") return !menu.tab.pinned;
  return menu.node.file ? !menu.node.file.pinned : false;
}
