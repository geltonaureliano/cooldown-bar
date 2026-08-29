import type { ProviderSnapshot, VisibleBounds } from "./types";

export function isStale(snapshot: ProviderSnapshot, now: number): boolean {
  const at = snapshot.observed_at;
  return snapshot.stale || at == null || !Number.isFinite(at) || at > now + 5
    || now - at >= snapshot.stale_after_seconds
    || (snapshot.primary?.resets_at != null && snapshot.primary.resets_at <= now);
}

export function readingStatus(snapshot: ProviderSnapshot, now: number): string {
  const at = snapshot.observed_at;
  if (at == null) return "Waiting for usage";
  const age = Math.max(0, Math.floor(now - at));
  const elapsed = age < 60 ? `${age}s ago` : age < 3600 ? `${Math.floor(age / 60)}m ago` : `${Math.floor(age / 3600)}h ago`;
  return `${snapshot.source === "file" ? "Local snapshot" : "CLI reading"} · ${elapsed}`;
}

export function menuPosition(x: number, y: number, width: number, height: number, viewportWidth: number, viewportHeight: number,
  bounds: VisibleBounds = { left: 0, top: 0, right: viewportWidth, bottom: viewportHeight }) {
  return { x: Math.max(bounds.left + 6, Math.min(x, bounds.right - width - 6)),
    y: Math.max(bounds.top + 6, Math.min(y, bounds.bottom - height - 6)) };
}
