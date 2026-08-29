import { readingStatus } from "../lib/freshness";
import { bubbleTailPath } from "../lib/silhouette";
import { ProviderMark } from "./icons/Marks";
import { clampPercent, resetText } from "../lib/format";
import type { ProviderSnapshot, UsageWindow } from "../lib/types";

function WindowRow({ window, color, now }: { window: UsageWindow; color: string; now: number }) {
  const pct = clampPercent(window.percent);
  return (
    <div className="bub-window">
      <div className="bub-row">
        <span className="bub-label">{window.label}</span>
        <span className="bub-reset">{resetText(window.resets_at, now)}</span>
      </div>
      <div className="bub-track">
        <div
          className="bub-fill"
          style={{ width: `${pct}%`, background: color }}
        />
      </div>
      <div className="bub-used">{Math.round(pct)}% Used</div>
    </div>
  );
}

/**
 * Detail bubble, anchored to the vertical centre of the hovered ring.
 *
 * `top` is clamped by the caller so a bubble for the first ring does not get
 * cut off by the top of the window; the tail stays on the ring regardless.
 */
export default function Bubble({
  snapshot,
  now,
  color,
  secondaryColor,
  top,
  tailY,
  leftEdge,
  icon,
  innerRef,
  tailWidth,
  tailHeight,
}: {
  snapshot: ProviderSnapshot;
  now: number;
  color: string;
  secondaryColor: string;
  top: number;
  tailY: number;
  /** True when the bar sits on the left edge, so the bubble opens rightward. */
  leftEdge: boolean;
  icon?: { url: string; vendor: boolean } | null;
  /** Measured by the caller to centre the bubble on the hovered ring. */
  innerRef?: React.Ref<HTMLDivElement>;
  tailWidth: number;
  tailHeight: number;
}) {
  const hasWindows = snapshot.primary || snapshot.secondary;

  return (
    <div
      ref={innerRef}
      className={`bubble${leftEdge ? " is-left" : ""}`}
      style={{ top }}
    >
      <div className="bub-head">
        <span className="bub-mark">
          <ProviderMark id={snapshot.id} size={15} color="#fff" icon={icon} />
        </span>
        <span className="bub-title">{snapshot.title}</span>
      </div>

      {snapshot.primary && <WindowRow window={snapshot.primary} color={color} now={now} />}
      {snapshot.secondary && (
        <WindowRow window={snapshot.secondary} color={secondaryColor} now={now} />
      )}

      {/* Say why a number is missing rather than showing an empty bubble. */}
      {snapshot.error && <div className="bub-error">{snapshot.error}</div>}
      {!hasWindows && !snapshot.error && (
        <div className="bub-error">No usage reported yet.</div>
      )}
      {snapshot.stale && hasWindows && (
        <div className="bub-stale">
          Last reading is out of date. Waiting for the provider.
        </div>
      )}

      {hasWindows && <div className="bub-freshness">{readingStatus(snapshot, now)}</div>}

      <svg
        className="bub-tail"
        width={tailWidth + 1}
        height={tailHeight}
        viewBox={`0 0 ${tailWidth + 1} ${tailHeight}`}
        style={{ top: tailY }}
        aria-hidden
      >
        <path d={bubbleTailPath(tailWidth + 1, tailHeight)} />
      </svg>
    </div>
  );
}
