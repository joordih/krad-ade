import { useEffect, useState } from "react";
import { pushState, turnPushOff, turnPushOn, type PushState } from "../../runtime/push";
import { Icon } from "../primitives/Icon";

const TITLES: Record<PushState, string> = {
  unsupported: "Add Krad to your home screen to get notifications",
  blocked: "Notifications are blocked in this browser's settings",
  off: "Notify me when an agent waits or finishes",
  on: "Notifications are on",
};

export function NotificationsButton() {
  const [state, setState] = useState<PushState>("off");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string>();

  useEffect(() => {
    void pushState().then(setState);
  }, []);

  async function toggle() {
    if (busy || state === "unsupported" || state === "blocked") return;
    setBusy(true);
    setProblem(undefined);
    try {
      setState(state === "on" ? await turnPushOff() : await turnPushOn());
    } catch (error) {
      setProblem(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <button
      className={state === "on" ? "round bell on" : "round bell"}
      title={problem ?? TITLES[state]}
      aria-label={TITLES[state]}
      onClick={() => void toggle()}
    >
      <Icon name={state === "on" ? "notification-01" : "notification-off-01"} size="sm" />
    </button>
  );
}
