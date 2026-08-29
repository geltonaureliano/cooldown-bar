/**
 * Reset countdown, formatted the way the design calls for:
 *
 *   under an hour  ->  "Resets in 51 min"
 *   under a day    ->  "Resets in 2h 15m"
 *   beyond that    ->  "Resets Thu 12:00 AM"
 */
export function resetText(resetsAt: number | null, now = Date.now() / 1000): string {
  if (resetsAt == null || !Number.isFinite(resetsAt)) return "";
  const deltaSeconds = resetsAt - now;
  if (deltaSeconds <= 0) return "Awaiting reset update";

  const minutes = Math.round(deltaSeconds / 60);
  if (minutes < 60) return `Resets in ${Math.max(1, minutes)} min`;

  if (deltaSeconds < 86_400) {
    const h = Math.floor(deltaSeconds / 3600);
    const m = Math.floor((deltaSeconds % 3600) / 60);
    return m === 0 ? `Resets in ${h}h` : `Resets in ${h}h ${m}m`;
  }

  const d = new Date(resetsAt * 1000);
  const weekday = d.toLocaleDateString(undefined, { weekday: "short" });
  const time = d.toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });
  return `Resets ${weekday} ${time}`;
}

/** Whole-percent label for the bar. `—` when there is nothing to show. */
export function percentLabel(percent: number | null | undefined): string {
  if (percent == null || !Number.isFinite(percent)) return "—";
  return `${Math.round(percent)}%`;
}

export function clampPercent(p: number | null | undefined): number {
  if (p == null || !Number.isFinite(p)) return 0;
  return Math.min(100, Math.max(0, p));
}
