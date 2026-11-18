import { contextWarns, windowLabel } from "./controls";

const RING_VIEW = 16;
const RING_RADIUS = 7;
const RING_CENTER = RING_VIEW / 2;
const FULL_PERCENT = 100;
const CONTEXT_LABEL = "Context";

export function UsageRing({ percent, window }: { percent: number | undefined; window: number | null }) {
  if (percent === undefined) return null;
  const circumference = 2 * Math.PI * RING_RADIUS;
  const title = `${CONTEXT_LABEL} ${percent}%`;
  return (
    <span className="usage" title={title}>
      <svg className={contextWarns(percent) ? "usage-ring warns" : "usage-ring"} viewBox={`0 0 ${RING_VIEW} ${RING_VIEW}`} aria-label={title}>
        <circle className="usage-track" cx={RING_CENTER} cy={RING_CENTER} r={RING_RADIUS} />
        <circle
          className="usage-fill"
          cx={RING_CENTER}
          cy={RING_CENTER}
          r={RING_RADIUS}
          strokeDasharray={circumference}
          strokeDashoffset={circumference * (1 - percent / FULL_PERCENT)}
        />
      </svg>
      {window !== null && <span className="usage-window">({windowLabel(window)})</span>}
    </span>
  );
}
