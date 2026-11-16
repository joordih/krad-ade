import { useRef, type PointerEvent as ReactPointerEvent } from "react";

export const LONG_PRESS_MS = 500;
export const LONG_PRESS_SLOP_PX = 10;

/** A press opens the menu once it has been held still long enough. */
export function longPressDue(heldMs: number, travelledPx: number): boolean {
  return heldMs >= LONG_PRESS_MS && travelledPx <= LONG_PRESS_SLOP_PX;
}

function travel(
  origin: { x: number; y: number } | undefined,
  current: { x: number; y: number },
): number {
  if (!origin) return 0;
  return Math.hypot(current.x - origin.x, current.y - origin.y);
}

/**
 * Touch and pen presses open the same menu a right click does. Mouse presses
 * are left to the context menu so a click stays a click.
 */
export function useLongPress(onLongPress: () => void) {
  const timer = useRef<number | undefined>(undefined);
  const origin = useRef<{ x: number; y: number } | undefined>(undefined);
  const travelled = useRef(0);
  const fired = useRef(false);

  const stop = () => {
    if (timer.current !== undefined) window.clearTimeout(timer.current);
    timer.current = undefined;
    origin.current = undefined;
  };

  const onPointerDown = (event: ReactPointerEvent) => {
    if (event.pointerType === "mouse") return;
    stop();
    fired.current = false;
    travelled.current = 0;
    origin.current = { x: event.clientX, y: event.clientY };
    const started = Date.now();
    timer.current = window.setTimeout(() => {
      if (!longPressDue(Date.now() - started, travelled.current)) return;
      fired.current = true;
      onLongPress();
    }, LONG_PRESS_MS);
  };

  const onPointerMove = (event: ReactPointerEvent) => {
    if (!origin.current) return;
    travelled.current = travel(origin.current, { x: event.clientX, y: event.clientY });
    if (travelled.current > LONG_PRESS_SLOP_PX) stop();
  };

  /** True once, for the click a completed long press leaves behind. */
  const consume = (): boolean => {
    const wasFired = fired.current;
    fired.current = false;
    return wasFired;
  };

  return { onPointerDown, onPointerMove, onPointerUp: stop, onPointerCancel: stop, consume };
}
