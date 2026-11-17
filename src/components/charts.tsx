import type { ReactNode } from "react";
import { Area, AreaChart, ResponsiveContainer, YAxis } from "recharts";
import { levelColor } from "../lib/colors";

/** Tiny trend line for stat cards. No axes; hover lives in the full charts. */
export function Sparkline({
  data,
  dataKey,
  color = "var(--accent)",
  height = 36,
  domain,
}: {
  data: object[];
  dataKey: string;
  color?: string;
  height?: number;
  domain?: [number, number | "auto"];
}) {
  const id = `spark-${dataKey}-${color.replace(/[^a-z0-9]/gi, "")}`;
  if (data.length < 2) return <div style={{ height }} />;
  return (
    <div style={{ height }} className="w-full" aria-hidden>
      <ResponsiveContainer width="100%" height="100%">
        <AreaChart data={data} margin={{ top: 2, right: 0, bottom: 0, left: 0 }}>
          <defs>
            <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor={color} stopOpacity={0.35} />
              <stop offset="100%" stopColor={color} stopOpacity={0} />
            </linearGradient>
          </defs>
          <YAxis hide domain={domain ?? [0, "auto"]} />
          <Area
            type="monotone"
            dataKey={dataKey}
            stroke={color}
            strokeWidth={1.5}
            fill={`url(#${id})`}
            isAnimationActive={false}
            connectNulls={false}
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}

/** Radial gauge (SVG). `value` null renders an empty track with the label. */
export function RingGauge({
  value,
  size = 96,
  stroke = 8,
  label,
  sub,
  warn = 75,
  crit = 90,
}: {
  value: number | null;
  size?: number;
  stroke?: number;
  label: ReactNode;
  sub?: ReactNode;
  warn?: number;
  crit?: number;
}) {
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  const sweep = 0.75; // 270° arc
  const v = value == null ? 0 : Math.max(0, Math.min(100, value));
  const color = levelColor(value, warn, crit);
  return (
    <div className="relative shrink-0" style={{ width: size, height: size }}>
      <svg width={size} height={size} className="rotate-[135deg]">
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="rgba(148,163,184,0.12)" strokeWidth={stroke} strokeDasharray={`${c * sweep} ${c}`} strokeLinecap="round" />
        {value != null && (
          <circle
            cx={size / 2}
            cy={size / 2}
            r={r}
            fill="none"
            stroke={color}
            strokeWidth={stroke}
            strokeLinecap="round"
            strokeDasharray={`${(c * sweep * v) / 100} ${c}`}
            style={{ transition: "stroke-dasharray 0.6s ease, stroke 0.3s", filter: `drop-shadow(0 0 6px color-mix(in srgb, ${color} 55%, transparent))` }}
          />
        )}
      </svg>
      <div className="absolute inset-0 flex flex-col items-center justify-center">
        <div className="font-mono text-[19px] font-semibold leading-none text-ink">{label}</div>
        {sub && <div className="mt-1 text-[10px] uppercase tracking-wider text-ink-3">{sub}</div>}
      </div>
    </div>
  );
}

interface TooltipRow {
  name: string;
  color: string;
  value: string;
}

/** Shared tooltip body: text in ink tokens, colour only on the swatch. */
export function TooltipBox({ title, rows, footer }: { title: string; rows: TooltipRow[]; footer?: string }) {
  return (
    <div className="glass rounded-lg px-3 py-2 text-[11.5px] shadow-xl">
      <div className="mb-1 font-medium text-ink-2">{title}</div>
      {rows.map((r) => (
        <div key={r.name} className="flex items-center justify-between gap-4">
          <span className="flex items-center gap-1.5 text-ink-2">
            <span className="inline-block h-2 w-2 rounded-sm" style={{ background: r.color }} />
            {r.name}
          </span>
          <span className="font-mono text-ink">{r.value}</span>
        </div>
      ))}
      {footer && <div className="mt-1 border-t border-line pt-1 text-ink-3">{footer}</div>}
    </div>
  );
}

export function Legend({
  items,
  onToggle,
}: {
  items: { name: string; color: string; key?: string; dimmed?: boolean }[];
  /** When set, items become buttons (e.g. to filter by series). */
  onToggle?: (key: string) => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-ink-2">
      {items.map((i) => {
        const inner = (
          <>
            <span className="inline-block h-2 w-2 rounded-sm" style={{ background: i.color }} />
            {i.name}
          </>
        );
        return onToggle ? (
          <button
            key={i.name}
            onClick={() => onToggle(i.key ?? i.name)}
            className={`accent-ring inline-flex items-center gap-1.5 rounded px-1 transition-opacity hover:text-ink ${i.dimmed ? "opacity-40" : ""}`}
          >
            {inner}
          </button>
        ) : (
          <span key={i.name} className="inline-flex items-center gap-1.5">
            {inner}
          </span>
        );
      })}
    </div>
  );
}
