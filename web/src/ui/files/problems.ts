import type { CodeDiagnostic } from "../../protocol/CodeDiagnostic";
import type { DiagnosticSeverity } from "../../protocol/DiagnosticSeverity";

export type ProblemSeverity = "error" | "warning";

const SEVERITY_RANK: Record<DiagnosticSeverity, number> = { error: 3, warning: 2, information: 1, hint: 0 };
const NO_PROBLEMS = "No problems";

function worse(left: DiagnosticSeverity, right: DiagnosticSeverity | undefined): boolean {
  return right === undefined || SEVERITY_RANK[left] > SEVERITY_RANK[right];
}

function counted(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`;
}

export function worstSeverity(diagnostics: readonly CodeDiagnostic[]): DiagnosticSeverity | undefined {
  return diagnostics.reduce<DiagnosticSeverity | undefined>(
    (worst, diagnostic) => (worse(diagnostic.severity, worst) ? diagnostic.severity : worst),
    undefined,
  );
}

export function problemSeverity(diagnostics: readonly CodeDiagnostic[]): ProblemSeverity | undefined {
  const worst = worstSeverity(diagnostics);
  return worst === "error" || worst === "warning" ? worst : undefined;
}

export function severityByLine(diagnostics: readonly CodeDiagnostic[]): Map<number, DiagnosticSeverity> {
  const lines = new Map<number, DiagnosticSeverity>();
  for (const diagnostic of diagnostics) {
    const line = diagnostic.range.start.line;
    if (worse(diagnostic.severity, lines.get(line))) lines.set(line, diagnostic.severity);
  }
  return lines;
}

export function countProblems(diagnostics: readonly CodeDiagnostic[]): { errors: number; warnings: number } {
  return {
    errors: diagnostics.filter((diagnostic) => diagnostic.severity === "error").length,
    warnings: diagnostics.filter((diagnostic) => diagnostic.severity === "warning").length,
  };
}

export function problemSummary(diagnostics: readonly CodeDiagnostic[]): string {
  const { errors, warnings } = countProblems(diagnostics);
  const parts: string[] = [];
  if (errors > 0) parts.push(counted(errors, "error", "errors"));
  if (warnings > 0) parts.push(counted(warnings, "warning", "warnings"));
  if (parts.length > 0) return parts.join(", ");
  return diagnostics.length > 0 ? counted(diagnostics.length, "problem", "problems") : NO_PROBLEMS;
}

export function sortedDiagnostics(diagnostics: readonly CodeDiagnostic[]): CodeDiagnostic[] {
  return [...diagnostics].sort(
    (a, b) =>
      a.range.start.line - b.range.start.line ||
      a.range.start.character - b.range.start.character ||
      SEVERITY_RANK[b.severity] - SEVERITY_RANK[a.severity],
  );
}
