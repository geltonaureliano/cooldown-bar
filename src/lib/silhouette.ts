/** Cubic control distance for a quarter circle. */
const KAPPA = 0.5522847498307936;

/**
 * Two tangent quarter ellipses join each end of the rail to the display edge.
 * The rail starts with zero width, opens to its full width, then closes back
 * into the bezel. The gutter remains transparent.
 */
export function barPath(
  width: number,
  height: number,
  radius: number,
  gutter: number,
  leftEdge: boolean,
): string {
  const inner = gutter;
  const outer = width;
  const mid = (inner + outer) / 2;
  const rx = (outer - inner) / 2;
  const ry = Math.max(0, Math.min(radius, height / 4));
  const x = (value: number) => leftEdge ? width - value : value;

  if (ry === 0) {
    return `M ${x(inner)},0 H ${x(outer)} V ${height} H ${x(inner)} Z`;
  }

  return [
    `M ${x(outer)},0`,
    `C ${x(outer)},${ry * KAPPA} ${x(mid + rx * KAPPA)},${ry} ${x(mid)},${ry}`,
    `C ${x(mid - rx * KAPPA)},${ry} ${x(inner)},${ry * (2 - KAPPA)} ${x(inner)},${2 * ry}`,
    `L ${x(inner)},${height - 2 * ry}`,
    `C ${x(inner)},${height - ry * (2 - KAPPA)} ${x(mid - rx * KAPPA)},${height - ry} ${x(mid)},${height - ry}`,
    `C ${x(mid + rx * KAPPA)},${height - ry} ${x(outer)},${height - ry * KAPPA} ${x(outer)},${height}`,
    `L ${x(outer)},0`,
    "Z",
  ].join(" ");
}

/** A broad curved attachment; the caller overlaps the body by one pixel. */
export function bubbleTailPath(width: number, height: number): string {
  const half = height / 2;
  return [
    "M 0,0",
    `C 0,${half * 0.62} ${width * 0.38},${half * 0.66} ${width},${half}`,
    `C ${width * 0.38},${half * 1.34} 0,${half * 1.38} 0,${height}`,
    "Z",
  ].join(" ");
}
