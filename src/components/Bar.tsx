import { useLayoutEffect, useRef } from "react";
import Ring from "./Ring";
import { clampPercent } from "../lib/format";
import { circleCurves, isDetached, liquidLevel, morphPath, orbDiameter, railCurves, spring, type MotionChannel } from "../lib/motion";
import type { Colors, Layout, MotionState, ProviderSnapshot } from "../lib/types";
import type { ResolvedIcon } from "../App";

/** A matched SVG outline morphs; native AppKit owns the actual window position. */
export default function Bar({ layout, snapshots, colors, icons, channel, motion }: {
  layout: Layout; snapshots: ProviderSnapshot[]; colors: Colors;
  icons: Record<string, ResolvedIcon | null>; channel: MotionChannel; motion: MotionState | null;
}) {
  const path = useRef<SVGPathElement>(null);
  const items = useRef<HTMLDivElement>(null);
  const orb = useRef<HTMLDivElement>(null);
  const shadow = useRef<HTMLDivElement>(null);
  const diameter = orbDiameter(layout.barWidth);
  const focus = snapshots[motion?.focusIndex ?? 0] ?? snapshots[0];
  const colorFor = (id: string) => id === "claude" ? colors.claude : id === "codex" ? colors.codex : colors.custom;
  const color = colorFor(focus?.id ?? "");
  const percent = focus?.primary ? clampPercent(focus.primary.percent) : null;
  // Always leave enough liquid to read as fluid at very low usage; its height
  // still follows the last known value without becoming another progress ring.
  const fill = liquidLevel(percent);

  useLayoutEffect(() => {
    const outline = path.current, rail = items.current, sphere = orb.current, shade = shadow.current;
    if (!outline || !rail || !sphere || !shade) return;
    const right = railCurves(layout.barWidth, layout.barHeight, layout.concaveRadius, "right");
    const left = railCurves(layout.barWidth, layout.barHeight, layout.concaveRadius, "left");
    const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
    let view = channel.get();
    let values = [view && isDetached(view) ? 1 : 0, view?.edge === "left" ? 1 : 0, 0, 0, 0];
    let velocities = [0, 0, 0, 0, 0];
    let frame = 0, previousTime = 0, disposed = false;

    const draw = () => {
      const p = values[0]!, side = values[1]!, vx = values[2]!, vy = values[3]!, magnet = values[4]!;
      const ax = view?.anchorX ?? layout.barWidth / 2, ay = view?.anchorY ?? layout.barHeight / 2;
      const speed = Math.min(1, Math.hypot(vx, vy));
      const angle = speed > 0.015 ? Math.atan2(vy, vx) : 0;
      const stretch = preference.matches ? 0 : speed * 0.075;
      const sx = 1 + stretch + magnet * 0.035, sy = 1 - stretch * 0.65;
      outline.setAttribute("d", morphPath(right, left, circleCurves(ax, ay, diameter / 2, sx, sy, angle), p, side));
      const reveal = Math.max(0, Math.min(1, (p - 0.22) / 0.65));
      rail.style.opacity = String(Math.pow(Math.max(0, 1 - p / 0.42), 2));
      rail.style.transformOrigin = `${ax}px ${ay}px`;
      rail.style.transform = `scale(${1 - p * 0.22}, ${1 - p * 0.75})`;
      for (const element of [sphere, shade]) {
        element.style.left = `${ax - diameter / 2}px`;
        element.style.top = `${ay - diameter / 2}px`;
        element.style.opacity = String(reveal);
      }
      // Only the small shadow/rim stretch. The glyph remains easy to read.
      shade.style.transform = `rotate(${angle}rad) scale(${sx}, ${sy})`;
      sphere.style.transform = `scale(${0.9 + reveal * 0.1})`;
      sphere.style.setProperty("--magnet", String(magnet));
    };
    const animate = (time: number) => {
      frame = 0;
      if (disposed) return;
      const dt = previousTime ? (time - previousTime) / 1000 : 1 / 60;
      previousTime = time;
      const targets = [view && isDetached(view) ? 1 : 0, view?.edge === "left" ? 1 : 0,
        preference.matches ? 0 : view?.velocityX ?? 0, preference.matches ? 0 : view?.velocityY ?? 0,
        preference.matches ? 0 : view?.magnet ?? 0];
      let settled = true;
      values = values.map((value, i) => {
        const target = targets[i]!;
        const [next, velocity] = preference.matches ? [target, 0] : spring(value, velocities[i]!, target, dt, i === 0 ? 19 : 24);
        velocities[i] = velocity;
        if (Math.abs(next - target) > 0.0005 || Math.abs(velocity) > 0.005) settled = false;
        return Math.abs(next - target) < 0.0005 && Math.abs(velocity) < 0.005 ? target : next;
      });
      draw();
      if (!settled) frame = requestAnimationFrame(animate);
      else { previousTime = 0; rail.style.willChange = ""; }
    };
    const wake = () => {
      if (!frame && !disposed) { rail.style.willChange = "transform, opacity"; frame = requestAnimationFrame(animate); }
    };
    draw();
    const unsubscribe = channel.subscribe((next) => { view = next; wake(); });
    preference.addEventListener("change", wake);
    return () => { disposed = true; unsubscribe(); preference.removeEventListener("change", wake); cancelAnimationFrame(frame); };
  }, [channel, layout.barWidth, layout.barHeight, layout.concaveRadius, diameter]);

  const paused = motion != null && motion.phase !== "docked";
  return <div className={`bar${paused ? " is-floating" : ""}`} style={{ width: layout.barWidth, height: layout.barHeight }}>
    <div ref={shadow} className="orb-shadow" style={{ width: diameter, height: diameter }} aria-hidden />
    <svg className="bar-surface" width={layout.barWidth} height={layout.barHeight} aria-hidden>
      <path ref={path} fill="#000" />
    </svg>
    <div ref={items} className="bar-items" aria-hidden={paused} style={{ paddingTop: layout.padY, paddingBottom: layout.padY, gap: layout.itemGap }}>
      {snapshots.map((s) => <Ring key={s.id} snapshot={s} color={colorFor(s.id)} diameter={layout.ringDiameter}
        lineWidth={layout.ringLineWidth} labelHeight={layout.labelHeight} labelGap={layout.labelGap} icon={icons[s.id] ?? null} />)}
    </div>
    <div ref={orb} className={`motion-orb${focus?.stale ? " is-stale" : ""}`}
      aria-hidden={!paused} aria-label="Usage updates paused while floating. Drag to a screen edge to resume."
      data-phase={motion?.phase ?? "docked"}
      style={{ width: diameter, height: diameter, "--liquid-level": `${fill}%`, "--liquid-primary": color,
        "--liquid-secondary": colors.codex } as React.CSSProperties}>
      <div className="orb-liquid" aria-hidden>
        <div className="liquid-body"><i className="liquid-glow one"/><i className="liquid-glow two"/></div>
        <div className="liquid-glass" />
      </div>
    </div>
  </div>;
}
