import { describe, expect, it } from "vitest";
import { controlOf, EXTRA_KEYS, gridOf, withStickyCtrl } from "./termKeys";

describe("extra terminal keys", () => {
  it("sends the escape sequences a person expects", () => {
    const byKey = Object.fromEntries(EXTRA_KEYS.map((extra) => [extra.key, extra.data]));
    expect(byKey.escape).toBe("\u001b");
    expect(byKey.tab).toBe("\t");
    expect(byKey.up).toBe("\u001b[A");
    expect(byKey.down).toBe("\u001b[B");
    expect(byKey.right).toBe("\u001b[C");
    expect(byKey.left).toBe("\u001b[D");
    expect(byKey["ctrl-c"]).toBe("\u0003");
  });
});

describe("sticky Ctrl", () => {
  it("maps letters and punctuation the way a terminal does", () => {
    expect(controlOf("a")).toBe("\u0001");
    expect(controlOf("A")).toBe("\u0001");
    expect(controlOf("z")).toBe("\u001a");
    expect(controlOf("[")).toBe("\u001b");
    expect(controlOf("?")).toBe("\u007f");
    expect(controlOf(" ")).toBe("\u0000");
  });

  it("only changes the next character while it is on", () => {
    expect(withStickyCtrl("c", true)).toBe("\u0003");
    expect(withStickyCtrl("cd", true)).toBe("\u0003d");
    expect(withStickyCtrl("c", false)).toBe("c");
    expect(withStickyCtrl("", true)).toBe("");
  });
});

describe("terminal grid", () => {
  it("never reports an empty grid", () => {
    expect(gridOf(80, 24, 8, 17)).toEqual({ cols: 80, rows: 24, cell_width_px: 8, cell_height_px: 17 });
    expect(gridOf(0, -1, -2, 0)).toEqual({ cols: 1, rows: 1, cell_width_px: 0, cell_height_px: 0 });
  });
});
