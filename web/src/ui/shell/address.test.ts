import { describe, expect, it } from "vitest";
import { hashOf, pairingCodeFromHash, placeFromHash } from "./address";

describe("addresses", () => {
  it("round-trips a session place through #s=", () => {
    const place = { runtime: "runner:2", environment: "main env", session: 4 };
    expect(placeFromHash(hashOf(place))).toEqual(place);
  });

  it("ignores anything that is not a whole place", () => {
    expect(placeFromHash("")).toBeUndefined();
    expect(placeFromHash("#s=local|main")).toBeUndefined();
    expect(placeFromHash("#s=local|main|x")).toBeUndefined();
  });

  it("reads a pairing code from #pair=", () => {
    expect(pairingCodeFromHash("#pair=ABCD1234")).toBe("ABCD1234");
    expect(pairingCodeFromHash("#s=local|main|1")).toBeUndefined();
  });
});
