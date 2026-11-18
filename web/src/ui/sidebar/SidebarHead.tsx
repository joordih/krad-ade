import { forgetPairing } from "../../runtime/http";
import { Icon } from "../primitives/Icon";
import { MenuItem, MenuSurface, MenuTrigger, useMenu } from "../primitives/Menu";
import { warn } from "../primitives/send";
import { NotificationsButton } from "./NotificationsButton";

const FORGET_QUESTION = "Forget this browser? You will need a new pairing code to use it again.";
const BROWSER_MENU = "This browser";

async function forget(): Promise<void> {
  if (!window.confirm(FORGET_QUESTION)) return;
  try {
    await forgetPairing();
    location.reload();
  } catch (error) {
    warn(error);
  }
}

export function SidebarHead({
  environment,
  environments,
  onEnvironment,
  attentionCount,
  onJump,
  wide,
}: {
  environment: string;
  environments: readonly string[];
  onEnvironment: (name: string) => void;
  attentionCount: number;
  onJump: () => void;
  wide: boolean;
}) {
  const browser = useMenu();
  return (
    <div className="sidebar-head">
      <label className="env-switch">
        <Icon name="layers-01" size="sm" />
        <select value={environment} onChange={(event) => onEnvironment(event.target.value)}>
          {environments.map((name) => (
            <option key={name} value={name}>
              {name}
            </option>
          ))}
        </select>
        <Icon name="arrow-down-01" size="sm" />
      </label>
      <div className="sidebar-tools">
        {attentionCount > 0 && (
          <button className="round jump" title="Jump to what needs attention" onClick={onJump}>
            <Icon name="target-02" size="sm" />
            <span className="attention-badge">{attentionCount}</span>
          </button>
        )}
        <NotificationsButton />
        <div className="head-menu">
          <MenuTrigger title={BROWSER_MENU} onClick={browser.show} className="head-more" />
          {browser.open && (
            <MenuSurface title={BROWSER_MENU} wide={wide} onClose={browser.close}>
              {(dismiss) => (
                <MenuItem
                  icon="plug-socket"
                  label="Forget this browser"
                  danger
                  onPick={() => {
                    dismiss();
                    void forget();
                  }}
                />
              )}
            </MenuSurface>
          )}
        </div>
      </div>
    </div>
  );
}
