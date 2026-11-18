import { describe, expect, it } from "vitest";
import { LONG_PRESS_MS, LONG_PRESS_SLOP_PX, longPressDue } from "./longPress";

describe("long press", () => {
  it("needs the full hold", () => {
    expect(longPressDue(LONG_PRESS_MS - 1, 0)).toBe(false);
    expect(longPressDue(LONG_PRESS_MS, 0)).toBe(true);
  });

  it("does not fire after a drag", () => {
    expect(longPressDue(LONG_PRESS_MS + 100, LONG_PRESS_SLOP_PX + 1)).toBe(false);
    expect(longPressDue(LONG_PRESS_MS + 100, LONG_PRESS_SLOP_PX)).toBe(true);
  });
});
