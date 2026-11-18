import { Icon } from "../../primitives/Icon";
import type { SendState } from "./controls";

const SEND_TOOLTIP = "Send to the agent";
const STOP_TOOLTIP = "Stop the agent";

export function SendButton({ state, onSend, onStop }: { state: SendState; onSend: () => void; onStop: () => void }) {
  const stops = state === "stop";
  const title = stops ? STOP_TOOLTIP : SEND_TOOLTIP;
  return (
    <button className={`send-button ${state}`} title={title} aria-label={title} disabled={state === "idle"} onClick={stops ? onStop : onSend}>
      <Icon name={stops ? "stop-circle" : "sent"} size="sm" />
    </button>
  );
}
