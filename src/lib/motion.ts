import type { Edge, MotionState } from "./types";

type Point = [number, number];
type Cubic = [Point, Point, Point, Point];
const K = 0.5522847498307936;
const mix = (a: number, b: number, t: number) => a + (b - a) * t;
const point = (a: Point, b: Point, t: number): Point => [mix(a[0], b[0], t), mix(a[1], b[1], t)];
const line = (a: Point, b: Point): Cubic => [a, point(a, b, 1 / 3), point(a, b, 2 / 3), b];

/** Same S-shaped rail as silhouette.ts, split into eight matched cubic arcs. */
export function railCurves(width: number, height: number, radius: number, edge: Edge): Cubic[] {
  const r = Math.max(0, Math.min(radius, height / 4)), m = width / 2;
  const curves: Cubic[] = [
    [[width, 0], [width, r * K], [m + m * K, r], [m, r]],
    [[m, r], [m - m * K, r], [0, r * (2 - K)], [0, 2 * r]],
    line([0, 2 * r], [0, height / 2]),
    line([0, height / 2], [0, height - 2 * r]),
    [[0, height - 2 * r], [0, height - r * (2 - K)], [m - m * K, height - r], [m, height - r]],
    [[m, height - r], [m + m * K, height - r], [width, height - r * K], [width, height]],
    line([width, height], [width, height / 2]),
    line([width, height / 2], [width, 0]),
  ];
  if (edge === "right") return curves;
  // Reverse the mirrored outline and rotate its start. Both sides now share
  // the circle's winding and landmarks, so switching edges never flips the orb.
  return [1, 0, 7, 6, 5, 4, 3, 2].map((i) =>
    [...curves[i]!].reverse().map(([x, y]) => [width - x, y] as Point) as Cubic);
}

export function circleCurves(x: number, y: number, radius: number, sx = 1, sy = 1, angle = 0): Cubic[] {
  const delta = -Math.PI / 4, k = 4 / 3 * Math.tan(delta / 4);
  const transform = (px: number, py: number): Point => [
    x + radius * (px * sx * Math.cos(angle) - py * sy * Math.sin(angle)),
    y + radius * (px * sx * Math.sin(angle) + py * sy * Math.cos(angle)),
  ];
  return Array.from({ length: 8 }, (_, i): Cubic => {
    const a = -Math.PI / 4 + delta * i, b = a + delta;
    return [transform(Math.cos(a), Math.sin(a)),
      transform(Math.cos(a) - k * Math.sin(a), Math.sin(a) + k * Math.cos(a)),
      transform(Math.cos(b) + k * Math.sin(b), Math.sin(b) - k * Math.cos(b)),
      transform(Math.cos(b), Math.sin(b))];
  });
}

export function morphPath(right: Cubic[], left: Cubic[], circle: Cubic[], detach: number, side: number): string {
  const t = Math.max(0, Math.min(1, detach)), e = Math.max(0, Math.min(1, side));
  const curves = right.map((curve, i) => curve.map((p, j) => point(point(p, left[i]![j]!, e), circle[i]![j]!, t)));
  const fmt = (p: Point) => `${p[0].toFixed(3)},${p[1].toFixed(3)}`;
  return `M ${fmt(curves[0]![0]!)} ${curves.map((c) => `C ${c.slice(1).map(fmt).join(" ")}`).join(" ")} Z`;
}

/** Analytic critical damping: stable at 30, 60 and 120 Hz, without overshoot. */
export function spring(value: number, velocity: number, target: number, dt: number, omega = 22): [number, number] {
  const step = Math.min(0.05, Math.max(0, dt));
  const change = value - target, temp = (velocity + omega * change) * step, decay = Math.exp(-omega * step);
  return [target + (change + temp) * decay, (velocity - omega * temp) * decay];
}

export const orbDiameter = (barWidth: number) => Math.min(76, Math.max(54, barWidth + 6));
export const liquidLevel = (percent: number | null | undefined) => percent == null || !Number.isFinite(percent)
  ? 46 : 24 + Math.min(100, Math.max(0, percent)) * 0.62;
export const isDetached = (state: MotionState) => state.phase === "dragging" || state.phase === "floating";

/** Motion frames bypass React; only phase/edge/provider changes need a render. */
export function createMotionChannel() {
  let current: MotionState | null = null;
  const listeners = new Set<(state: MotionState) => void>();
  return {
    get: () => current,
    publish(next: MotionState): boolean {
      if (current && next.revision <= current.revision) return false;
      current = next;
      listeners.forEach((listener) => listener(next));
      return true;
    },
    subscribe(listener: (state: MotionState) => void) {
      listeners.add(listener);
      if (current) listener(current);
      return () => { listeners.delete(listener); };
    },
  };
}
export type MotionChannel = ReturnType<typeof createMotionChannel>;
