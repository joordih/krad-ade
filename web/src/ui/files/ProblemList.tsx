import type { CodeDiagnostic } from "../../protocol/CodeDiagnostic";
import { lineId } from "./CodeLines";
import { problemSummary } from "./problems";

export function ProblemList({ problems }: { problems: readonly CodeDiagnostic[] }) {
  if (problems.length === 0) return null;
  return (
    <div className="problems">
      <div className="problems-head">
        <span>Problems</span>
        <span className="detail">{problemSummary(problems)}</span>
      </div>
      <div className="problems-list">
        {problems.map((problem, index) => (
          <button
            key={index}
            className={`problem-row ${problem.severity}`}
            onClick={() => document.getElementById(lineId(problem.range.start.line))?.scrollIntoView({ block: "center" })}
          >
            <span className={`problem-dot ${problem.severity}`} />
            <span className="problem-place">
              {problem.range.start.line + 1}:{problem.range.start.character + 1}
            </span>
            <span className="problem-message">{problem.message}</span>
            {problem.source && <span className="problem-source">{problem.source}</span>}
          </button>
        ))}
      </div>
    </div>
  );
}
