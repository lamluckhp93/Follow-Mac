import { LineChart as LineIcon } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Area, AreaChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { api, errorMessage } from "../lib/api";
import { AXIS, GRID, SERIES } from "../lib/colors";
import { fmtBytes, fmtPct, fmtRate } from "../lib/format";
import { useT } from "../lib/i18n";
import { SYSTEM_RANGES } from "../lib/ranges";
import type { MetricPoint } from "../lib/types";
import { Legend, TooltipBox } from "./charts";
import { Card, Empty, Notice, Segmented } from "./ui";

type MetricKey = "cpu" | "memory" | "network" | "disk" | "gpu";

interface SeriesDef {
  key: keyof MetricPoint | "memPct";
  name: string;
  color: string;
}

// One unit per chart (never a dual axis): each metric gets its own chart.
const METRICS: Record<MetricKey, { label: string; series: SeriesDef[]; fmt: (v: number) => string; percent?: boolean; stacked?: boolean }> = {
  cpu: {
    label: "CPU",
    series: [
      { key: "cpuUser", name: "User", color: SERIES.a },
      { key: "cpuSystem", name: "System", color: SERIES.b },
    ],
    fmt: (v) => fmtPct(v, 1),
    percent: true,
    stacked: true,
  },
  memory: { label: "Memory", series: [{ key: "memPct", name: "Used", color: SERIES.a }], fmt: (v) => fmtPct(v, 1), percent: true },
  network: {
    label: "Network",
    series: [
      { key: "netRxBps", name: "Download", color: SERIES.a },
      { key: "netTxBps", name: "Upload", color: SERIES.b },
    ],
    fmt: fmtRate,
  },
  disk: {
    label: "Disk I/O",
    series: [
      { key: "diskReadBps", name: "Read", color: SERIES.a },
      { key: "diskWriteBps", name: "Write", color: SERIES.b },
    ],
    fmt: fmtRate,
  },
  gpu: { label: "GPU", series: [{ key: "gpuUsage", name: "Utilisation", color: SERIES.a }], fmt: (v) => fmtPct(v, 1), percent: true },
};

const fmtAxisTime = (minutes: number) => (ts: number) => {
  const d = new Date(ts);
  return minutes <= 15
    ? d.toLocaleTimeString(undefined, { minute: "2-digit", second: "2-digit" })
    : d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
};

export function PerformanceChart() {
  const t = useT();
  const [minutes, setMinutes] = useState<number>(5);
  const [metric, setMetric] = useState<MetricKey>("cpu");
  const [points, setPoints] = useState<MetricPoint[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    const load = () =>
      api
        .systemHistory(minutes)
        .then((p) => alive && (setPoints(p), setError(null)))
        .catch((e) => alive && setError(errorMessage(e)));
    load();
    const id = setInterval(load, minutes <= 15 ? 2000 : minutes <= 60 ? 10_000 : 30_000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [minutes]);

  const def = METRICS[metric];
  const data = useMemo(
    () => points.map((p) => ({ ...p, memPct: p.memUsed != null && p.memTotal ? (p.memUsed / p.memTotal) * 100 : null })),
    [points],
  );
  const hasData = data.some((d) => def.series.some((s) => (d as Record<string, unknown>)[s.key] != null));

  return (
    <Card
      title={t("section.performance")}
      icon={<LineIcon size={15} />}
      right={
        <>
          <Segmented
            value={metric}
            onChange={setMetric}
            options={(Object.keys(METRICS) as MetricKey[]).map((k) => ({ key: k, label: METRICS[k].label }))}
          />
          <Segmented value={minutes} onChange={setMinutes} options={SYSTEM_RANGES.map((r) => ({ key: r.key, label: r.label }))} />
        </>
      }
    >
      {error && <Notice tone="error">{error}</Notice>}
      <div className="mb-2 flex items-center justify-between">
        {def.series.length > 1 ? <Legend items={def.series.map((s) => ({ name: s.name, color: s.color }))} /> : <span />}
        <span className="text-[11px] text-ink-3">
          {points.length} points · {minutes <= 15 ? "live samples" : "stored history (SQLite)"}
        </span>
      </div>
      <div className="h-[240px]">
        {!hasData ? (
          <Empty title="No samples in this range yet">
            History is recorded while Mac Observer runs. Longer ranges fill in over time.
          </Empty>
        ) : (
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={data} margin={{ top: 6, right: 8, bottom: 0, left: 0 }}>
              <defs>
                {def.series.map((s) => (
                  <linearGradient key={s.key} id={`pc-${s.key}`} x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stopColor={s.color} stopOpacity={0.3} />
                    <stop offset="100%" stopColor={s.color} stopOpacity={0.02} />
                  </linearGradient>
                ))}
              </defs>
              <CartesianGrid stroke={GRID} vertical={false} />
              <XAxis
                dataKey="ts"
                type="number"
                scale="time"
                domain={["dataMin", "dataMax"]}
                tickFormatter={fmtAxisTime(minutes)}
                stroke={AXIS}
                tickLine={false}
                axisLine={false}
                minTickGap={48}
              />
              <YAxis
                width={64}
                stroke={AXIS}
                tickLine={false}
                axisLine={false}
                domain={def.percent ? [0, 100] : [0, "auto"]}
                tickFormatter={(v: number) => (def.percent ? `${v}%` : metric === "memory" ? fmtBytes(v) : def.fmt(v))}
              />
              <Tooltip
                cursor={{ stroke: "rgba(148,163,184,0.35)", strokeWidth: 1 }}
                content={({ active, payload, label }) =>
                  active && payload?.length ? (
                    <TooltipBox
                      title={new Date(Number(label)).toLocaleTimeString()}
                      rows={def.series.map((s) => {
                        const v = (payload[0].payload as Record<string, number | null>)[s.key];
                        return { name: s.name, color: s.color, value: v == null ? "N/A" : def.fmt(v) };
                      })}
                      footer={
                        def.stacked
                          ? `Total ${fmtPct(def.series.reduce((a, s) => a + ((payload[0].payload as Record<string, number>)[s.key] ?? 0), 0), 1)}`
                          : undefined
                      }
                    />
                  ) : null
                }
              />
              {def.series.map((s) => (
                <Area
                  key={s.key}
                  type="monotone"
                  dataKey={s.key}
                  name={s.name}
                  stackId={def.stacked ? "a" : undefined}
                  stroke={s.color}
                  strokeWidth={2}
                  fill={`url(#pc-${s.key})`}
                  isAnimationActive={false}
                  connectNulls={false}
                  activeDot={{ r: 4, strokeWidth: 2, stroke: "#0f1526" }}
                />
              ))}
            </AreaChart>
          </ResponsiveContainer>
        )}
      </div>
    </Card>
  );
}
