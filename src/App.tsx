import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import Bar from "./components/Bar";
import Bubble from "./components/Bubble";
import { convertFileSrc } from "@tauri-apps/api/core";
import ContextMenu from "./components/ContextMenu";
import {
  getUsage,
  onHover,
  onState,
  onMotion,
  dockNearest,
  setMotionPreferences,
  onMenuClose,
  requestRefresh,
  quitApp,
  reloadConfig,
  setMenuOpen,
} from "./lib/ipc";
import { createMotionChannel } from "./lib/motion";
import { isStale } from "./lib/freshness";
import { subscribe } from "./lib/subscriptions";
import type { Bootstrap, Colors, IconSource, Layout, MotionState, ProviderSnapshot } from "./lib/types";

export interface ResolvedIcon {
  url: string;
  vendor: boolean;
}

/** Keep the bubble from being clipped by the window edge. */
const EDGE_PAD = 8;

export default function App() {
  const [channel] = useState(createMotionChannel);
  const [motion, setMotion] = useState<MotionState | null>(null);
  const motionKey = useRef("");
  const [layout, setLayout] = useState<Layout | null>(null);
  const [readings, setSnapshots] = useState<ProviderSnapshot[]>([]);
  const [colors, setColors] = useState<Colors | null>(null);
  const [iconPaths, setIconPaths] = useState<Record<string, IconSource>>({});
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [hover, setHover] = useState({ index: -1, centerY: 0 });
  const bubbleRef = useRef<HTMLDivElement | null>(null);
  const [bubbleHeight, setBubbleHeight] = useState(140);
  const [now, setNow] = useState(() => Date.now() / 1000);
  const [configError, setConfigError] = useState<string | null>(null);
  const [ipcError, setIpcError] = useState<string | null>(null);
  const revision = useRef(-1);
  const snapshots = useMemo(() => readings.map((s) => ({ ...s, stale: isStale(s, now) })), [readings, now]);
  const reportError = useCallback((error: unknown) => setIpcError(String(error)), []);
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now() / 1000), 1000);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    let alive = true;
    const applyMotion = (m: MotionState) => {
      if (!alive || !channel.publish(m)) return;
      const key = `${m.phase}:${m.edge}:${m.focusIndex}`;
      if (key !== motionKey.current) {
        motionKey.current = key;
        setMotion(m);
        if (m.phase !== "docked") setHover({ index: -1, centerY: 0 });
      }
    };
    const apply = (b: Bootstrap) => {
      if (!alive || b.revision < revision.current) return;
      revision.current = b.revision;
      applyMotion(b.motion);
      setLayout(b.layout);
      setSnapshots(b.snapshots);
      setColors(b.colors);
      setIconPaths(b.icons);
      setConfigError(b.configError);
      setIpcError(null);
    };
    const cleanup = subscribe([
      () => onState(apply),
      () => onMotion(applyMotion),
      () => onHover((h) => { if (alive) setHover(h); }),
      () => onMenuClose(() => { if (alive) setMenu(null); }),
    ], async () => apply(await getUsage()), reportError);
    return () => { alive = false; cleanup(); };
  }, [reportError, channel]);

  useEffect(() => {
    const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => void setMotionPreferences(preference.matches).catch(reportError);
    update();
    preference.addEventListener("change", update);
    return () => preference.removeEventListener("change", update);
  }, [reportError]);

  // Measure the bubble itself, not its wrapper.
  //
  // The wrapper is `position: absolute; inset: 0`, so measuring that returned
  // the full window height — which made `rawTop` negative every time and pinned
  // the bubble to the top edge, with only the tail appearing to move.
  useLayoutEffect(() => {
    const el = bubbleRef.current;
    if (!el) return;
    const measure = () => setBubbleHeight(el.offsetHeight);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [hover.index, snapshots, layout]);

  const onContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setMenu({ x: e.clientX, y: e.clientY });
    void setMenuOpen(true).catch(reportError);
  }, [reportError]);

  const closeMenu = useCallback(() => {
    setMenu(null);
    void setMenuOpen(false).catch(reportError);
  }, [reportError]);

  const hovered = useMemo(
    () =>
      hover.index >= 0 && hover.index < snapshots.length
        ? snapshots[hover.index]
        : null,
    [hover.index, snapshots]
  );

  // A PNG dropped at ~/.notchusage/icons/<id>.png replaces the drawn mark.
  // Served through Tauri's asset protocol, which tauri.conf.json scopes to that
  // one directory.
  const icons = useMemo<Record<string, ResolvedIcon | null>>(() => {
    const out: Record<string, ResolvedIcon | null> = {};
    for (const [id, src] of Object.entries(iconPaths)) {
      out[id] = { url: `${convertFileSrc(src.path)}?v=${src.version}`, vendor: src.vendor };
    }
    return out;
  }, [iconPaths]);

  if (!layout || !colors) return ipcError ? <div className="startup-error" role="alert">NotchUsage: {ipcError}</div> : null;

  const leftEdge = (motion?.edge ?? layout.edge) === "left";
  const paused = motion != null && motion.phase !== "docked";
  const barX = (layout.windowWidth - layout.barWidth) / 2;
  const gutter = layout.nodeWidth - layout.barWidth;
  const bubbleSpace = layout.bubbleWidth + layout.bubbleTailWidth + 10;
  const visible = channel.get()?.visible;
  const bounds = visible && visible.right > visible.left && visible.bottom > visible.top ? visible : undefined;

  // Centred on the hovered ring. `hover.centerY` is window-relative and the
  // window now has transparent slack above and below the bar, so this lands
  // exactly on the ring instead of being clamped to the window edge — which is
  // what previously made the bubble look pinned with only its tail moving.
  const rawTop = hover.centerY - bubbleHeight / 2;
  const maxTop = Math.max(EDGE_PAD, layout.windowHeight - bubbleHeight - EDGE_PAD);
  const top = Math.min(Math.max(rawTop, EDGE_PAD), maxTop);
  // The tail stays pointing at the ring even when the body has been clamped.
  const tailY = Math.min(
    Math.max(hover.centerY - top - layout.bubbleTailHeight / 2, layout.bubbleRadius),
    Math.max(layout.bubbleRadius, bubbleHeight - layout.bubbleTailHeight - layout.bubbleRadius)
  );

  const colorFor = (id: string) =>
    id === "claude" ? colors.claude : id === "codex" ? colors.codex : colors.custom;

  return (
    <div
      className={`root${leftEdge ? " is-left" : ""}`}
      onContextMenu={onContextMenu}
      style={{
        "--bubble-width": `${layout.bubbleWidth}px`,
        "--bubble-tail-width": `${layout.bubbleTailWidth}px`,
        "--bubble-radius": `${layout.bubbleRadius}px`,
      } as CSSProperties}
    >
      {/*
        Everything beside the bar is transparent, and it cannot swallow clicks:
        the panel turns on `ignoresMouseEvents` whenever the cursor is not over
        the bar itself. See mouse.rs.
      */}
      <div className="bubble-layer" style={{ width: bubbleSpace, left: leftEdge ? barX + layout.barWidth + gutter : barX - gutter - bubbleSpace }}>
        {hovered && !paused && (
          <div className="bubble-anchor">
            <Bubble
              innerRef={bubbleRef}
              snapshot={hovered}
              now={now}
              color={colorFor(hovered.id)}
              secondaryColor={colors.codex}
              top={top}
              tailY={tailY}
              tailWidth={layout.bubbleTailWidth}
              tailHeight={layout.bubbleTailHeight}
              leftEdge={leftEdge}
              icon={icons[hovered.id] ?? null}
            />
          </div>
        )}
      </div>

      {/*
        The window reserves `barOffsetY` of transparent slack above the bar so
        the bubble can centre on the first ring. Rust positions the window that
        much higher to compensate, so the bar must be pushed down by the same
        amount here — otherwise it renders at the top of the window and ends up
        off the top of the screen.
      */}
      <div
        className="bar-layer"
        style={{ width: layout.barWidth, height: layout.barHeight, left: barX, top: layout.barOffsetY }}
      >
        <Bar layout={layout} snapshots={snapshots} colors={colors} icons={icons} channel={channel} motion={motion} />
      </div>

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          error={ipcError ?? configError}
          paused={paused}
          bounds={bounds}
          onDock={() => void dockNearest().catch(reportError)}
          onRefresh={() => void requestRefresh().catch(reportError)}
          onReload={() => void reloadConfig().catch(reportError)}
          onQuit={() => void quitApp().catch(reportError)}
          onClose={closeMenu}
        />
      )}
    </div>
  );
}
