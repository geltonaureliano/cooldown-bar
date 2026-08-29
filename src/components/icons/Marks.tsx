/**
 * Provider marks, drawn here rather than bundled.
 *
 * No vendor logo is shipped or fetched. The Claude mark is the one shape the
 * design specifies exactly; the others are neutral geometry. A user who wants
 * the real logos can drop a PNG at `~/.notchusage/icons/<id>.png` and it will be
 * used instead — see `ProviderMark` below.
 */

interface MarkProps {
  size: number;
  color?: string;
}

/**
 * Twelve spokes radiating from the centre, alternating full and 72% length,
 * stroke width ~14% of the radius.
 */
export function ClaudeMark({ size, color = "#fff" }: MarkProps) {
  const r = size / 2;
  const stroke = Math.max(1, r * 0.115);
  const inner = r * 0.1;
  const spokes = Array.from({ length: 12 }, (_, i) => {
    const angle = (i * 30 * Math.PI) / 180;
    const len = (i % 2 === 0 ? 1 : 0.72) * (r - stroke * 0.5);
    return {
      x1: r + Math.cos(angle) * inner,
      y1: r + Math.sin(angle) * inner,
      x2: r + Math.cos(angle) * len,
      y2: r + Math.sin(angle) * len,
    };
  });

  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden>
      {spokes.map((s, i) => (
        <line
          key={i}
          x1={s.x1}
          y1={s.y1}
          x2={s.x2}
          y2={s.y2}
          stroke={color}
          strokeWidth={stroke}
          strokeLinecap="round"
        />
      ))}
    </svg>
  );
}

/** Neutral mark for Codex: a hexagonal knot outline. */
export function CodexMark({ size, color = "#fff" }: MarkProps) {
  const r = size / 2;
  const stroke = Math.max(1, r * 0.13);
  const ring = r * 0.62;
  const pts = Array.from({ length: 6 }, (_, i) => {
    const a = ((i * 60 - 90) * Math.PI) / 180;
    return `${(r + Math.cos(a) * ring).toFixed(2)},${(r + Math.sin(a) * ring).toFixed(2)}`;
  }).join(" ");

  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden>
      <polygon
        points={pts}
        fill="none"
        stroke={color}
        strokeWidth={stroke}
        strokeLinejoin="round"
      />
      <circle cx={r} cy={r} r={r * 0.17} fill={color} />
    </svg>
  );
}

/** Neutral mark for a custom provider: a four-point star. */
export function CustomMark({ size, color = "#fff" }: MarkProps) {
  const r = size / 2;
  const o = r * 0.66;
  const i = r * 0.2;
  const d = [
    `M ${r} ${r - o}`,
    `Q ${r + i} ${r - i} ${r + o} ${r}`,
    `Q ${r + i} ${r + i} ${r} ${r + o}`,
    `Q ${r - i} ${r + i} ${r - o} ${r}`,
    `Q ${r - i} ${r - i} ${r} ${r - o}`,
    "Z",
  ].join(" ");
  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden>
      <path d={d} fill={color} />
    </svg>
  );
}

/**
 * Pick the mark for a provider id, honouring an icon file when one exists.
 *
 * Icon files are served through Tauri's asset protocol, scoped in
 * `tauri.conf.json` to `~/.notchusage/icons/**` and nothing else.
 */
export function ProviderMark({
  id,
  size,
  color,
  icon,
}: {
  id: string;
  size: number;
  color?: string;
  icon?: { url: string; vendor: boolean } | null;
}) {
  if (icon) {
    // A vendor app icon is a dark glyph on a light rounded square. Inverting it
    // gives a light glyph on a dark square, and `screen` blending drops that now
    // near-black background into the disc behind it — leaving just the mark, as
    // in the reference. A user-supplied icon is drawn exactly as given.
    const style: React.CSSProperties = icon.vendor
      ? {
          display: "block",
          objectFit: "contain",
          // `invert` alone leaves a mid-grey glyph, because the source mark is
          // navy rather than black. The brightness push finishes the job.
          filter: "invert(1) brightness(1.9) contrast(1.15)",
          mixBlendMode: "screen",
        }
      : { display: "block", objectFit: "cover", borderRadius: "50%" };
    return <img src={icon.url} width={size} height={size} alt="" style={style} />;
  }
  if (id === "claude") return <ClaudeMark size={size} color={color} />;
  if (id === "codex") return <CodexMark size={size} color={color} />;
  return <CustomMark size={size} color={color} />;
}
