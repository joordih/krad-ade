import { describe, expect, it } from "vitest";
import type { ToolCall } from "../../../protocol/ToolCall";
import { blocksOf, doneSummary, oneLine, rowVerb } from "./tools";

function call(kind: ToolCall["kind"], name = kind, summary = ""): ToolCall {
  return { name, kind, summary };
}

describe("tool groups", () => {
  it("folds consecutive tool calls into one group between other parts", () => {
    const blocks = blocksOf([{ text: "a" }, { tool: call("Shell") }, { tool: call("Read") }, { text: "b" }, { tool: call("Edit") }]);
    expect(blocks.map((block) => (block.kind === "tools" ? block.calls.length : "part"))).toEqual(["part", 2, "part", 1]);
  });

  it("sums a finished group like the desktop", () => {
    expect(doneSummary([call("Shell"), call("Shell"), call("Read")])).toBe("Ran 2 commands, read 1 file");
    expect(doneSummary([call("Search")])).toBe("Ran 1 search");
  });

  it("names each row by its kind and keeps the first line of its summary", () => {
    expect(rowVerb(call("Shell"))).toBe("Ran");
    expect(rowVerb({ name: "mcp__docs", kind: "Other", summary: "" })).toBe("mcp__docs");
    expect(oneLine("cargo test\n--all")).toBe("cargo test…");
    expect(oneLine("  ls  ")).toBe("ls");
  });
});
