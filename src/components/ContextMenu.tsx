import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { VisibleBounds } from "../lib/types";
import { menuPosition } from "../lib/freshness";

/** HTML menu: native NSMenu tracking can block a non-key NSPanel. */
export default function ContextMenu({ x, y, error, paused, bounds, onDock, onRefresh, onReload, onQuit, onClose }: {
  x: number; y: number; error: string | null; paused: boolean; bounds?: VisibleBounds; onDock: () => void;
  onRefresh: () => void; onReload: () => void; onQuit: () => void; onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement | null>(null);
  const [position, setPosition] = useState({ x, y });
  useLayoutEffect(() => {
    const place = () => {
      const box = ref.current?.getBoundingClientRect();
      if (box) setPosition(menuPosition(x, y, box.width, box.height, window.innerWidth, window.innerHeight, bounds));
    };
    place();
    window.addEventListener("resize", place);
    return () => window.removeEventListener("resize", place);
  }, [x, y, error, paused, bounds]);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  const run = (action: () => void) => { onClose(); action(); };
  return <>
    <div className="menu-scrim" onMouseDown={onClose} />
    <div ref={ref} className="ctxmenu" role="menu" style={{ left: position.x, top: position.y, maxWidth: bounds ? bounds.right - bounds.left - 12 : undefined, maxHeight: bounds ? bounds.bottom - bounds.top - 12 : undefined }}>
      {paused && <div className="ctxmenu-status">Updates paused while floating</div>}
      {paused && <button type="button" role="menuitem" className="ctxmenu-item" onClick={() => run(onDock)}>Attach to nearest edge</button>}
      {error && <div className="ctxmenu-error" role="alert">{error}</div>}
      <button type="button" role="menuitem" className="ctxmenu-item" disabled={paused} onClick={() => run(onRefresh)}>Refresh usage</button>
      <button type="button" role="menuitem" className="ctxmenu-item" onClick={() => run(onReload)}>Reload config</button>
      <div className="ctxmenu-sep" />
      <button type="button" role="menuitem" className="ctxmenu-item" onClick={onQuit}>Quit Cooldown Bar</button>
    </div>
  </>;
}
