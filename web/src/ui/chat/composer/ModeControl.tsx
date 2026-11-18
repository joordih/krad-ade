import type { PermissionMode } from "../../../protocol/PermissionMode";
import { Icon } from "../../primitives/Icon";
import { MenuItem, MenuSection, MenuSurface, useMenu } from "../../primitives/Menu";
import { ControlPill } from "./ControlPill";
import { PERMISSION_MODES, permissionLabel } from "./controls";

const MODE_TITLE = "Permission mode";

export function ModeControl({ mode, wide, onMode }: { mode: PermissionMode | null | undefined; wide: boolean; onMode: (mode: PermissionMode) => void }) {
  const menu = useMenu();
  return (
    <div className="control">
      <ControlPill title={MODE_TITLE} open={menu.open} onClick={menu.show}>
        <Icon name="square-lock-02" size="xs" tone="muted" />
        <span className="control-label">{permissionLabel(mode)}</span>
        <Icon name="arrow-down-01" size="xs" tone="subtle" />
      </ControlPill>
      {menu.open && (
        <MenuSurface title={MODE_TITLE} wide={wide} onClose={menu.close} placement="above-start">
          {(dismiss) => (
            <>
              {wide && <MenuSection>{MODE_TITLE}</MenuSection>}
              {PERMISSION_MODES.map((known) => (
                <MenuItem
                  key={known}
                  label={permissionLabel(known)}
                  checked={mode === known}
                  onPick={() => {
                    onMode(known);
                    dismiss();
                  }}
                />
              ))}
            </>
          )}
        </MenuSurface>
      )}
    </div>
  );
}
