import { ProviderMark } from "./icons/Marks";
import { clampPercent, percentLabel } from "../lib/format";
import type { ProviderSnapshot } from "../lib/types";

/**
 * One progress ring.
 *
 * Drawn as inline SVG rather than a CSS `conic-gradient` because the design
 * needs a round line cap on the progress arc, which conic gradients cannot do.
 */
export default function Ring({
  snapshot,
  color,
  diameter,
  lineWidth,
  labelHeight,
  labelGap,
  icon,
}: {
  snapshot: ProviderSnapshot;
  color: string;
  diameter: number;
  lineWidth: number;
  labelHeight: number;
  labelGap: number;
  icon?: { url: string; vendor: boolean } | null;
}) {
  const percent = snapshot.primary ? clampPercent(snapshot.primary.percent) : null;
  // Stale or missing data must never look like a confident zero.
  const dimmed = snapshot.stale || percent == null;

  // Broad charcoal track, with a thinner coloured arc centred on it.
  const trackWidth = Math.max(lineWidth, diameter * 0.13);
  const r = Math.max(0, (diameter - trackWidth) / 2);
  const c = 2 * Math.PI * r;
  const offset = c * (1 - (percent ?? 0) / 100);
  const center = diameter / 2;
  // Proportions taken off the reference: the inner disc is about 64% of the
  // ring's diameter, which leaves a clear band of bar-black between the disc and
  // the track. Sizing the disc off the stroke width instead closed that gap to a
  // couple of points and the ring read as one heavy grey donut.
  const discRadius = diameter * 0.32;
  // A drawn mark sits inside the disc; a vendor image fills it edge to edge.
  const glyphSize = Math.round(diameter * 0.36);
  const iconSize = icon ? Math.round(discRadius * 1.75) : glyphSize;

  return (
    <div className={`ring-item${dimmed ? " is-dimmed" : ""}`} aria-label={`${snapshot.title}: ${dimmed ? "reading unavailable or outdated" : percentLabel(percent)}`}>
      <div className="ring" style={{ width: diameter, height: diameter }}>
        <svg width={diameter} height={diameter} aria-hidden>
          {/* The centre is continuous with the black rail. */}
          <circle cx={center} cy={center} r={discRadius} fill="#000" />
          <circle
            cx={center}
            cy={center}
            r={r}
            fill="none"
            stroke="#2d2d2d"
            strokeWidth={trackWidth}
          />
          <circle
            className="ring-progress"
            cx={center}
            cy={center}
            r={r}
            fill="none"
            stroke={color}
            opacity={percent === 0 || percent == null ? 0 : 1}
            strokeWidth={lineWidth}
            strokeLinecap="round"
            strokeDasharray={`${c} ${c}`}
            strokeDashoffset={offset}
            /* Start at 12 o'clock and run clockwise. */
            transform={`rotate(-90 ${center} ${center})`}
          />
        </svg>
        <span className="ring-icon">
          <ProviderMark
            id={snapshot.id}
            size={iconSize}
            color="#fff"
            icon={icon}
          />
        </span>
      </div>
      <div
        className="ring-label"
        style={{ height: labelHeight, marginTop: labelGap }}
      >
        {percentLabel(dimmed ? null : percent)}
      </div>
    </div>
  );
}
