import { describe, expect, it } from "vitest";
import type { CodeDiagnostic } from "../../protocol/CodeDiagnostic";
import { countProblems, problemSeverity, problemSummary, severityByLine, sortedDiagnostics } from "./problems";

function diagnostic(line: number, severity: CodeDiagnostic["severity"], message = "problem", character = 0): CodeDiagnostic {
  return {
    range: { start: { line, character }, end: { line, character: character + 1 } },
    severity,
    message,
    source: "rust-analyzer",
  };
}

describe("problems", () => {
  it("marks errors over warnings and counts only what the tree shows", () => {
    const diagnostics = [diagnostic(4, "hint"), diagnostic(2, "warning"), diagnostic(2, "error"), diagnostic(7, "information")];
    expect(problemSeverity(diagnostics)).toBe("error");
    expect(problemSeverity([diagnostic(0, "information")])).toBeUndefined();
    expect(countProblems(diagnostics)).toEqual({ errors: 1, warnings: 1 });
    expect(problemSummary(diagnostics)).toBe("1 error, 1 warning");
    expect(problemSummary([diagnostic(0, "error"), diagnostic(1, "error")])).toBe("2 errors");
    expect(problemSummary([diagnostic(0, "hint")])).toBe("1 problem");
    expect(problemSummary([])).toBe("No problems");
    expect(Object.fromEntries(severityByLine(diagnostics))).toEqual({ 2: "error", 4: "hint", 7: "information" });
  });

  it("sorts problems by line and column, worst first on a line", () => {
    const diagnostics = [
      diagnostic(9, "warning", "late"),
      diagnostic(1, "warning", "second", 4),
      diagnostic(1, "error", "first", 4),
      diagnostic(1, "hint", "early", 0),
    ];
    expect(sortedDiagnostics(diagnostics).map((problem) => problem.message)).toEqual(["early", "first", "second", "late"]);
  });
});
