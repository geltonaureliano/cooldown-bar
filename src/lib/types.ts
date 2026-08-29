export type Source = "cli" | "file" | "unavailable";

export interface UsageWindow {
  /** 0..100 */
  percent: number;
  /** Absolute unix seconds, or null when the provider did not say. */
  resets_at: number | null;
  label: string;
}

export interface ProviderSnapshot {
  id: string;
  title: string;
  primary: UsageWindow | null;
  secondary: UsageWindow | null;
  stale: boolean;
  error: string | null;
  source: Source;
  available: boolean;
  plan: string | null;
  observed_at: number | null;
  checked_at: number;
  stale_after_seconds: number;
}

export type Edge = "left" | "right";

export interface Layout {
  barWidth: number;
  nodeWidth: number;
  barHeight: number;
  windowWidth: number;
  windowHeight: number;
  barOffsetY: number;
  concaveRadius: number;
  ringDiameter: number;
  ringLineWidth: number;
  itemGap: number;
  padY: number;
  labelHeight: number;
  labelGap: number;
  bubbleWidth: number;
  bubbleTailWidth: number;
  bubbleTailHeight: number;
  bubbleRadius: number;
  itemCount: number;
  edge: Edge;
}

export interface Colors {
  claude: string;
  codex: string;
  custom: string;
}

export interface IconSource {
  path: string;
  /**
   * True for an icon copied out of a vendor's app bundle: a dark glyph on a
   * light rounded square, which gets inverted to the flat light-on-dark mark the
   * design calls for. User-supplied icons are rendered untouched.
   */
  vendor: boolean;
  version: number;
}

export interface Bootstrap {
  motion: MotionState;
  revision: number;
  layout: Layout;
  snapshots: ProviderSnapshot[];
  colors: Colors;
  configError: string | null;
  /** provider id -> icon file to draw instead of the built-in mark. */
  icons: Record<string, IconSource>;
}

export interface HoverState {
  /** Index of the hovered ring, or -1 for none. */
  index: number;
  centerY: number;
}

export interface VisibleBounds { left: number; top: number; right: number; bottom: number; }
export interface MotionState {
  revision: number;
  phase: "docked" | "dragging" | "floating" | "docking";
  edge: Edge;
  anchorX: number;
  anchorY: number;
  focusIndex: number;
  magnet: number;
  targetEdge: Edge | null;
  velocityX: number;
  velocityY: number;
  over: boolean;
  visible: VisibleBounds;
}
