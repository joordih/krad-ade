import { describe, expect, it } from "vitest";
import { choiceDetail, hostLabel, runOnChoices, runOnOffered } from "./runOn";

describe("the run-on chooser", () => {
  it("lists this computer first, then each runner with its state", () => {
    const choices = runOnChoices(
      hostLabel({ kind: "local" }),
      [
        { id: 1, name: "box", destination: "me@box" },
        { id: 2, name: "gone", destination: "me@gone" },
      ],
      [
        { runner: 1, state: { state: "ready", linger: {} as never } },
        { runner: 2, state: { state: "failed", reason: "x" } },
      ],
    );
    expect(choices.map((choice) => [choice.label, choiceDetail(choice), choice.open])).toEqual([
      ["This computer", undefined, true],
      ["box", "Ready", true],
      ["gone", "Failed · Reconnect it in Settings to use it", false],
    ]);
  });

  it("is offered only for a fresh local chat whose group has a remote, with runners set up", () => {
    expect(runOnOffered({ runners: 1, fresh: true, cloudSession: false, remote: true })).toBe(true);
    expect(runOnOffered({ runners: 0, fresh: true, cloudSession: false, remote: true })).toBe(false);
    expect(runOnOffered({ runners: 1, fresh: false, cloudSession: false, remote: true })).toBe(false);
    expect(runOnOffered({ runners: 1, fresh: true, cloudSession: true, remote: true })).toBe(false);
    expect(runOnOffered({ runners: 1, fresh: true, cloudSession: false, remote: false })).toBe(false);
  });
});
