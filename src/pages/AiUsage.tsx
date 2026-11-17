import { open } from "@tauri-apps/plugin-dialog";
import { BarChart3, Coins, FileUp, Filter, FlaskConical, KeyRound, Network, RefreshCw, Settings2, Sparkles, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useState, type KeyboardEvent as ReactKeyboardEvent, type ReactNode } from "react";
import { Bar, BarChart, CartesianGrid, Cell, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { Legend, TooltipBox } from "../components/charts";
import { Badge, Button, Card, cx, Empty, KV, Notice, Segmented, SourceBadge, StatusPill, TextInput } from "../components/ui";
import { api, errorMessage } from "../lib/api";
import { AXIS, GRID, PROVIDER_COLORS, PROVIDER_LABEL, PROVIDER_ORDER, SERIES, STATUS } from "../lib/colors";
import { fmtAgo, fmtDateTime, fmtInt, fmtTokens, fmtUsd } from "../lib/format";
import { useT } from "../lib/i18n";
import { resolveRange, type RangeKey } from "../lib/ranges";
import type { Analytics, Breakdown, ImportReport, ProviderId, ProviderView, Totals, UsageRecord } from "../lib/types";
import { useAi } from "../store/ai";

type T = ReturnType<typeof useT>;

const RANGES: RangeKey[] = ["today", "yesterday", "7d", "30d", "month", "custom"];

interface FilterState {
  providers: ProviderId[];
  models: string[];
}
const NO_FILTER: FilterState = { providers: [], models: [] };

function toggle<V>(list: V[], v: V): V[] {
  return list.includes(v) ? list.filter((x) => x !== v) : [...list, v];
}

function isoDay(d: Date) {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

function totalInput(t: Totals) {
  return t.input + t.cached + t.cacheWrite;
}

function canSync(p: ProviderView) {
  return p.enabled && p.status !== "not_configured" && (p.capabilities.usageApi || p.capabilities.costApi || p.capabilities.accountInfo);
}

function Kpi({ label, value, sub, title }: { label: string; value: string; sub?: ReactNode; title?: string }) {
  return (
    <div className="glass card-pad animate-in rounded-2xl px-4 py-3" title={title}>
      <div className="text-[10.5px] uppercase tracking-wider text-ink-3">{label}</div>
      <div className="mt-1 font-mono text-[22px] font-semibold leading-tight text-ink">{value}</div>
      {sub && <div className="mt-0.5 text-[11px] text-ink-3">{sub}</div>}
    </div>
  );
}

function ProviderCard({
  p,
  b,
  selected,
  dimmed,
  onSelect,
  onConfigure,
}: {
  p: ProviderView;
  b?: Breakdown;
  selected: boolean;
  dimmed: boolean;
  onSelect: () => void;
  onConfigure: () => void;
}) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ tone: "good" | "error"; text: string } | null>(null);
  const configured = p.status !== "not_configured";
  const tot = b?.totals;

  const sync = async () => {
    setBusy(true);
    setMsg(null);
    try {
      setMsg({ tone: "good", text: await api.syncProvider(p.id) });
    } catch (e) {
      setMsg({ tone: "error", text: errorMessage(e) });
    } finally {
      setBusy(false);
    }
  };

  const onKey = (e: ReactKeyboardEvent) => {
    if (e.target !== e.currentTarget) return;
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onSelect();
    }
  };

  return (
    <div
      role="button"
      tabIndex={0}
      aria-pressed={selected}
      title={t("ai.card.filter")}
      onClick={onSelect}
      onKeyDown={onKey}
      className={cx(
        "accent-ring flex cursor-pointer rounded-2xl transition-[opacity,box-shadow,transform] hover:-translate-y-px",
        selected && "shadow-[0_0_0_1.5px_var(--accent)]",
        dimmed && "opacity-55 hover:opacity-90",
      )}
    >
      <Card
        className="flex-1"
        title={
          <span className="flex items-center gap-2">
            <span className="h-2.5 w-2.5 rounded-sm" style={{ background: PROVIDER_COLORS[p.id] }} />
            <span className="text-ink">{p.displayName}</span>
          </span>
        }
        right={<StatusPill status={p.status} />}
      >
        <div className="mb-2 flex flex-wrap gap-1.5">
          <Badge tone={p.hasApiKey ? "good" : "neutral"}>
            <KeyRound size={10} /> API key {p.hasApiKey ? "✓" : "—"}
          </Badge>
          {p.capabilities.adminKeyLabel && (
            <Badge tone={p.hasAdminKey ? "good" : "neutral"} title={p.capabilities.adminKeyLabel}>
              <KeyRound size={10} /> Admin {p.hasAdminKey ? "✓" : "—"}
            </Badge>
          )}
          {!p.capabilities.usageApi && <Badge title={p.capabilities.notes}>No usage API</Badge>}
        </div>
        {tot && tot.total > 0 ? (
          <>
            <KV k={t("ai.kpi.input")} v={fmtTokens(tot.input + tot.cacheWrite)} />
            <KV k="Cached" v={fmtTokens(tot.cached)} />
            <KV k={t("ai.kpi.output")} v={fmtTokens(tot.output)} />
            <KV k={t("ai.kpi.total")} v={fmtTokens(tot.total)} />
            <KV k={t("ai.kpi.actual")} v={tot.actualCost > 0 ? fmtUsd(tot.actualCost) : "N/A"} />
            <KV k={t("ai.kpi.estimated")} v={tot.estimatedCost > 0 ? fmtUsd(tot.estimatedCost) : tot.unpricedTokens > 0 ? "no price" : "—"} />
          </>
        ) : tot && tot.actualCost > 0 ? (
          <KV k={t("ai.kpi.actual")} v={fmtUsd(tot.actualCost)} />
        ) : (
          <div className="py-2 text-[12px] text-ink-3">{configured ? t("ai.card.noUsage") : t("ai.card.addKey")}</div>
        )}
        {p.account && (
          <div className="mt-1 border-t border-line pt-1">
            {p.account.balanceLabel && <KV k="Account" v={p.account.balanceLabel} />}
            {p.account.balanceUsd != null && <KV k="Balance / remaining" v={fmtUsd(p.account.balanceUsd)} />}
            {p.account.usedTodayUsd != null && <KV k="Credits used today" v={fmtUsd(p.account.usedTodayUsd)} />}
            {p.account.usedMonthUsd != null && <KV k="Credits used this month" v={fmtUsd(p.account.usedMonthUsd)} />}
          </div>
        )}
        {b && b.sources.length > 0 && (
          <div className="mt-2 flex flex-wrap gap-1">
            {b.sources.map((s) => (
              <SourceBadge key={s} source={s} />
            ))}
            {tot && tot.estimatedCost > 0 && <SourceBadge source="estimated" />}
          </div>
        )}
        {p.lastError && p.status === "error" && (
          <div className="mt-2">
            <Notice tone="error">{p.lastError}</Notice>
          </div>
        )}
        {msg && (
          <div className="mt-2">
            <Notice tone={msg.tone}>{msg.text}</Notice>
          </div>
        )}
        {/* Buttons must not toggle the filter. */}
        <div className="mt-auto flex items-center justify-between gap-2 pt-3" onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()}>
          <span className="text-[11px] text-ink-3" title={p.lastSyncAt ? new Date(p.lastSyncAt).toString() : undefined}>
            {t("ai.card.lastSync")} {p.lastSyncAt ? fmtAgo(p.lastSyncAt) : t("ai.card.never")}
          </span>
          <div className="flex gap-1.5">
            {canSync(p) && (
              <Button variant="ghost" onClick={sync} loading={busy || p.status === "syncing"} title={t("ai.card.sync")} aria-label={t("ai.card.sync")}>
                <RefreshCw size={13} />
              </Button>
            )}
            <Button variant="ghost" onClick={onConfigure} title={t("ai.card.configure")} aria-label={t("ai.card.configure")}>
              <Settings2 size={13} />
            </Button>
          </div>
        </div>
      </Card>
    </div>
  );
}

interface HRow {
  id: string;
  name: string;
  value: number;
  color: string;
  detail?: string;
}

function HBarChart({
  rows,
  valueFmt,
  emptyLabel,
  active,
  onSelect,
}: {
  rows: HRow[];
  valueFmt: (v: number) => string;
  emptyLabel: string;
  /** Ids currently selected; others are dimmed when non-empty. */
  active?: string[];
  onSelect?: (row: HRow) => void;
}) {
  if (rows.length === 0) return <Empty title={emptyLabel} />;
  const anyActive = (active?.length ?? 0) > 0;
  return (
    <div style={{ height: Math.max(120, rows.length * 30 + 20) }}>
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={rows} layout="vertical" margin={{ top: 0, right: 64, bottom: 0, left: 0 }} barCategoryGap={6}>
          <XAxis type="number" hide domain={[0, "auto"]} />
          <YAxis type="category" dataKey="name" width={170} tickLine={false} axisLine={false} stroke={AXIS} tick={{ fontSize: 11 }} />
          <Tooltip
            cursor={{ fill: "rgba(148,163,184,0.06)" }}
            content={({ active: a, payload }) =>
              a && payload?.length ? (
                <TooltipBox
                  title={String(payload[0].payload.name)}
                  rows={[{ name: payload[0].payload.detail ?? "Value", color: payload[0].payload.color, value: valueFmt(Number(payload[0].value)) }]}
                />
              ) : null
            }
          />
          <Bar
            dataKey="value"
            radius={[0, 4, 4, 0]}
            isAnimationActive={false}
            cursor={onSelect ? "pointer" : undefined}
            onClick={onSelect ? (d: unknown) => onSelect((d as { payload: HRow }).payload) : undefined}
            label={{ position: "right", fill: "#a3adc4", fontSize: 11, formatter: (v: unknown) => valueFmt(Number(v)) }}
          >
            {rows.map((r) => (
              <Cell key={r.id} fill={r.color} fillOpacity={anyActive && !active!.includes(r.id) ? 0.3 : 1} />
            ))}
          </Bar>
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

function UsageOverTime({
  a,
  monthly,
  metric,
  selected,
  onToggleProvider,
  t,
}: {
  a: Analytics;
  monthly: boolean;
  metric: "tokens" | "cost";
  selected: ProviderId[];
  onToggleProvider: (p: ProviderId) => void;
  t: T;
}) {
  const series = monthly ? a.monthly : a.daily;
  if (series.length === 0) return <Empty title={t("ai.chart.empty")} />;

  if (metric === "cost") {
    const items = [
      { key: "actualCost", name: t("ai.chart.actual"), color: SERIES.a },
      { key: "estimatedCost", name: t("ai.chart.estimated"), color: STATUS.warning },
    ];
    const data = series.map((d) => ({ label: d.label, actualCost: d.actualCost, estimatedCost: d.estimatedCost }));
    if (!data.some((d) => d.actualCost > 0 || d.estimatedCost > 0)) return <Empty title={t("ai.chart.empty")} />;
    return (
      <>
        <div className="mb-2">
          <Legend items={items} />
        </div>
        <div className="h-[230px]">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={data} margin={{ top: 4, right: 8, bottom: 0, left: 0 }} barCategoryGap="22%">
              <CartesianGrid stroke={GRID} vertical={false} />
              <XAxis dataKey="label" stroke={AXIS} tickLine={false} axisLine={false} minTickGap={16} />
              <YAxis stroke={AXIS} tickLine={false} axisLine={false} width={56} tickFormatter={(v: number) => fmtUsd(v)} />
              <Tooltip
                cursor={{ fill: "rgba(148,163,184,0.06)" }}
                content={({ active, payload, label }) =>
                  active && payload?.length ? (
                    <TooltipBox
                      title={`${label} (UTC)`}
                      rows={items.map((it) => ({ name: it.name, color: it.color, value: fmtUsd((payload[0].payload as Record<string, number>)[it.key]) }))}
                    />
                  ) : null
                }
              />
              {items.map((it, i) => (
                <Bar key={it.key} dataKey={it.key} stackId="c" fill={it.color} stroke="#0f1526" strokeWidth={1} radius={i === items.length - 1 ? [4, 4, 0, 0] : 0} isAnimationActive={false} />
              ))}
            </BarChart>
          </ResponsiveContainer>
        </div>
      </>
    );
  }

  const providers = PROVIDER_ORDER.filter((p) => series.some((d) => (d.tokensByProvider[p] ?? 0) > 0));
  const data = series.map((d) => ({ label: d.label, ...d.tokensByProvider }));
  if (providers.length === 0) return <Empty title={t("ai.chart.empty")} />;
  const dim = (p: ProviderId) => selected.length > 0 && !selected.includes(p);
  return (
    <>
      <div className="mb-2">
        <Legend
          items={providers.map((p) => ({ key: p, name: PROVIDER_LABEL[p], color: PROVIDER_COLORS[p], dimmed: dim(p) }))}
          onToggle={(k) => onToggleProvider(k as ProviderId)}
        />
      </div>
      <div className="h-[230px]">
        <ResponsiveContainer width="100%" height="100%">
          <BarChart data={data} margin={{ top: 4, right: 8, bottom: 0, left: 0 }} barCategoryGap="22%">
            <CartesianGrid stroke={GRID} vertical={false} />
            <XAxis dataKey="label" stroke={AXIS} tickLine={false} axisLine={false} minTickGap={16} />
            <YAxis stroke={AXIS} tickLine={false} axisLine={false} width={56} tickFormatter={(v: number) => fmtTokens(v)} />
            <Tooltip
              cursor={{ fill: "rgba(148,163,184,0.06)" }}
              content={({ active, payload, label }) =>
                active && payload?.length ? (
                  <TooltipBox
                    title={`${label} (UTC)`}
                    rows={providers
                      .filter((p) => (payload[0].payload as Record<string, number>)[p])
                      .map((p) => ({ name: PROVIDER_LABEL[p], color: PROVIDER_COLORS[p], value: fmtTokens((payload[0].payload as Record<string, number>)[p]) }))}
                    footer={`Total ${fmtTokens(providers.reduce((s, p) => s + ((payload[0].payload as Record<string, number>)[p] ?? 0), 0))}`}
                  />
                ) : null
              }
            />
            {providers.map((p, i) => (
              <Bar
                key={p}
                dataKey={p}
                stackId="t"
                fill={PROVIDER_COLORS[p]}
                fillOpacity={dim(p) ? 0.3 : 1}
                stroke="#0f1526"
                strokeWidth={1}
                radius={i === providers.length - 1 ? [4, 4, 0, 0] : 0}
                isAnimationActive={false}
                cursor="pointer"
                onClick={() => onToggleProvider(p)}
              />
            ))}
          </BarChart>
        </ResponsiveContainer>
      </div>
    </>
  );
}

function InputOutput({ a, t }: { a: Analytics; t: T }) {
  const data = a.daily.map((d) => ({ label: d.label, input: d.input, cached: d.cached, output: d.output }));
  if (!data.some((d) => d.input + d.cached + d.output > 0)) return <Empty title={t("ai.chart.empty")} />;
  const items = [
    { key: "input", name: t("ai.kpi.input"), color: SERIES.a },
    { key: "cached", name: "Cached", color: SERIES.c },
    { key: "output", name: t("ai.kpi.output"), color: SERIES.b },
  ];
  return (
    <>
      <div className="mb-2">
        <Legend items={items} />
      </div>
      <div className="h-[230px]">
        <ResponsiveContainer width="100%" height="100%">
          <BarChart data={data} margin={{ top: 4, right: 8, bottom: 0, left: 0 }} barCategoryGap="22%">
            <CartesianGrid stroke={GRID} vertical={false} />
            <XAxis dataKey="label" stroke={AXIS} tickLine={false} axisLine={false} minTickGap={16} />
            <YAxis stroke={AXIS} tickLine={false} axisLine={false} width={56} tickFormatter={(v: number) => fmtTokens(v)} />
            <Tooltip
              cursor={{ fill: "rgba(148,163,184,0.06)" }}
              content={({ active, payload, label }) =>
                active && payload?.length ? (
                  <TooltipBox
                    title={`${label} (UTC)`}
                    rows={items.map((it) => ({ name: it.name, color: it.color, value: fmtTokens((payload[0].payload as Record<string, number>)[it.key]) }))}
                  />
                ) : null
              }
            />
            {items.map((it, i) => (
              <Bar key={it.key} dataKey={it.key} stackId="io" fill={it.color} stroke="#0f1526" strokeWidth={1} radius={i === items.length - 1 ? [4, 4, 0, 0] : 0} isAnimationActive={false} />
            ))}
          </BarChart>
        </ResponsiveContainer>
      </div>
    </>
  );
}

function TelemetryLog({ version, filter, t }: { version: number; filter: FilterState; t: T }) {
  const [limit, setLimit] = useState(15);
  const [rows, setRows] = useState<UsageRecord[] | null>(null);
  const filtering = filter.providers.length > 0 || filter.models.length > 0;
  useEffect(() => {
    // Fetch a larger window while filtering so filtered rows are not starved.
    api
      .recentTelemetry(filtering ? Math.min(500, limit * 10) : limit + 1)
      .then(setRows)
      .catch(() => setRows([]));
  }, [version, limit, filtering]);
  if (!rows) return null;
  const visible = rows.filter(
    (r) => (filter.providers.length === 0 || filter.providers.includes(r.provider)) && (filter.models.length === 0 || filter.models.includes(r.model)),
  );
  if (visible.length === 0)
    return (
      <Empty title={t("ai.telemetry.empty")}>
        {t("ai.telemetry.emptyHint")}
      </Empty>
    );
  return (
    <>
      <table className="w-full text-[12px]">
        <thead>
          <tr className="text-left text-[10.5px] uppercase tracking-wider text-ink-3">
            <th className="pb-1 font-medium">Time</th>
            <th className="pb-1 font-medium">Provider</th>
            <th className="pb-1 font-medium">Model</th>
            <th className="pb-1 text-right font-medium">{t("ai.kpi.input")}</th>
            <th className="pb-1 text-right font-medium">Cached</th>
            <th className="pb-1 text-right font-medium">{t("ai.kpi.output")}</th>
            <th className="pb-1 text-right font-medium">{t("ai.chart.cost")}</th>
          </tr>
        </thead>
        <tbody>
          {visible.slice(0, limit).map((r) => (
            <tr key={r.externalId} className="border-t border-line">
              <td className="py-1 text-ink-2">{fmtDateTime(r.bucketStart)}</td>
              <td className="py-1">
                <span className="inline-flex items-center gap-1.5 text-ink">
                  <span className="h-2 w-2 rounded-sm" style={{ background: PROVIDER_COLORS[r.provider] }} />
                  {PROVIDER_LABEL[r.provider]}
                </span>
              </td>
              <td className="max-w-[220px] truncate py-1 font-mono text-ink-2">{r.model}</td>
              <td className="py-1 text-right font-mono text-ink">{fmtInt(r.inputTokens + r.cacheWriteTokens)}</td>
              <td className="py-1 text-right font-mono text-ink-2">{fmtInt(r.cachedTokens)}</td>
              <td className="py-1 text-right font-mono text-ink">{fmtInt(r.outputTokens)}</td>
              <td className="py-1 text-right font-mono text-ink-2">{r.costUsd != null ? fmtUsd(r.costUsd) : "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {visible.length > limit && limit < 500 && (
        <div className="mt-2 flex justify-center">
          <Button variant="ghost" onClick={() => setLimit((l) => Math.min(500, l + 25))}>
            {t("ai.telemetry.more")}
          </Button>
        </div>
      )}
    </>
  );
}

function Onboarding({ t, onSettings, onImport }: { t: T; onSettings: () => void; onImport: () => void }) {
  const steps = [
    { icon: <KeyRound size={16} />, title: t("ai.onboard.key"), hint: t("ai.onboard.keyHint"), onClick: onSettings },
    { icon: <FlaskConical size={16} />, title: t("ai.onboard.test"), hint: t("ai.onboard.testHint"), onClick: onSettings },
    { icon: <FileUp size={16} />, title: t("ai.onboard.import"), hint: t("ai.onboard.importHint"), onClick: onImport },
    { icon: <Network size={16} />, title: t("ai.onboard.proxy"), hint: t("ai.onboard.proxyHint"), onClick: onSettings },
  ];
  return (
    <Card title={t("ai.onboard.title")} icon={<Sparkles size={15} />}>
      <p className="mb-3 text-[12px] text-ink-2">{t("ai.onboard.body")}</p>
      <div className="grid grid-cols-1 gap-2 md:grid-cols-2 xl:grid-cols-4">
        {steps.map((s, i) => (
          <button
            key={i}
            onClick={s.onClick}
            className="accent-ring group flex items-start gap-3 rounded-xl border border-line bg-surface-2/50 p-3 text-left transition-colors hover:border-line-strong hover:bg-surface-2"
          >
            <span className="accent-text mt-0.5 flex shrink-0">{s.icon}</span>
            <span className="min-w-0">
              <span className="block text-[12.5px] font-medium text-ink group-hover:text-ink">
                {i + 1}. {s.title}
              </span>
              <span className="mt-0.5 block text-[11.5px] leading-snug text-ink-3">{s.hint}</span>
            </span>
          </button>
        ))}
      </div>
    </Card>
  );
}

export default function AiUsage({ onOpenSettings }: { onOpenSettings: (provider?: ProviderId) => void }) {
  const t = useT();
  const providers = useAi((s) => s.providers);
  const usageVersion = useAi((s) => s.usageVersion);
  const [range, setRange] = useState<RangeKey>("7d");
  const [custom, setCustom] = useState(() => {
    const now = new Date();
    return { from: isoDay(new Date(now.getTime() - 6 * 86_400_000)), to: isoDay(now) };
  });
  const [filter, setFilter] = useState<FilterState>(NO_FILTER);
  const [metric, setMetric] = useState<"tokens" | "cost">("tokens");
  const [a, setA] = useState<Analytics | null>(null);
  /** Unfiltered analytics, so provider cards keep their numbers while filtering. */
  const [all, setAll] = useState<Analytics | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const [report, setReport] = useState<ImportReport | null>(null);
  const [syncingAll, setSyncingAll] = useState(false);
  const [syncReport, setSyncReport] = useState<{ provider: ProviderId; ok: boolean; text: string }[] | "none" | null>(null);

  const filtering = filter.providers.length > 0 || filter.models.length > 0;

  useEffect(() => {
    let alive = true;
    const [from, to] = resolveRange(range, new Date(), custom);
    if (!(to > from)) return;
    setLoading(true);
    Promise.all([api.analytics(from, to, filtering ? filter : undefined), filtering ? api.analytics(from, to) : null])
      .then(([x, unfiltered]) => {
        if (!alive) return;
        setA(x);
        setAll(unfiltered ?? x);
        setError(null);
      })
      .catch((e) => alive && setError(errorMessage(e)))
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [range, custom, usageVersion, filter, filtering]);

  // Esc clears the filter.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      setFilter(NO_FILTER);
      // A clicked chart keeps focus; Esc would then show its ring around the plot.
      const el = document.activeElement;
      if (el instanceof SVGElement || el?.closest?.(".recharts-wrapper")) (el as HTMLElement | SVGElement).blur();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const toggleProvider = useCallback((p: ProviderId) => setFilter((f) => ({ ...f, providers: toggle(f.providers, p) })), []);
  const toggleModel = useCallback((m: string) => setFilter((f) => ({ ...f, models: toggle(f.models, m) })), []);

  const byProvider = useMemo(() => new Map(all?.byProvider.map((b) => [b.provider, b]) ?? []), [all]);
  const spanDays = a ? (a.to - a.from) / 86_400_000 : 0;
  const noneConfigured = providers.length > 0 && providers.every((p) => p.status === "not_configured");
  const showOnboarding = noneConfigured && !filtering && a != null && a.recordCount === 0;

  const doImport = async () => {
    setError(null);
    let path: string | string[] | null;
    try {
      path = await open({ multiple: false, filters: [{ name: "Usage data", extensions: ["csv", "json"] }] });
    } catch (e) {
      setError(errorMessage(e));
      return;
    }
    if (!path || Array.isArray(path)) return;
    setImporting(true);
    setReport(null);
    try {
      setReport(await api.importUsageFile(path));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setImporting(false);
    }
  };

  const syncAll = async () => {
    const targets = providers.filter(canSync);
    if (targets.length === 0) {
      setSyncReport("none");
      return;
    }
    setSyncingAll(true);
    setSyncReport(null);
    const results = await Promise.allSettled(targets.map((p) => api.syncProvider(p.id)));
    setSyncReport(
      results.map((r, i) => ({
        provider: targets[i].id,
        ok: r.status === "fulfilled",
        text: r.status === "fulfilled" ? r.value : errorMessage(r.reason),
      })),
    );
    setSyncingAll(false);
  };

  const totals = a?.totals;
  // Comparison charts show everything (selection is dimmed, so it can be
  // extended by clicking); models are restricted to the selected providers.
  const providerOk = (p: ProviderId) => filter.providers.length === 0 || filter.providers.includes(p);
  const modelsAll = (all?.byModel ?? []).filter((m) => providerOk(m.provider));
  const modelRows: HRow[] = modelsAll
    .filter((m) => m.totals.total > 0)
    .slice(0, 10)
    .map((m) => ({ id: m.key, name: m.key, value: m.totals.total, color: PROVIDER_COLORS[m.provider], detail: `${PROVIDER_LABEL[m.provider]} · tokens` }));
  const costProviderRows: HRow[] = (all?.byProvider ?? [])
    .map((b) => ({
      id: b.provider,
      name: PROVIDER_LABEL[b.provider],
      value: b.totals.actualCost + b.totals.estimatedCost,
      color: PROVIDER_COLORS[b.provider],
      detail: b.totals.estimatedCost > 0 ? (b.totals.actualCost > 0 ? "actual + estimated" : "estimated") : "actual",
    }))
    .filter((r) => r.value > 0);
  const costModelRows: HRow[] = modelsAll
    .map((m) => ({
      id: m.key,
      name: m.key,
      value: m.totals.actualCost + m.totals.estimatedCost,
      color: PROVIDER_COLORS[m.provider],
      detail: `${PROVIDER_LABEL[m.provider]} · ${m.totals.estimatedCost > 0 ? (m.totals.actualCost > 0 ? "actual + estimated" : "estimated") : "actual"}`,
    }))
    .filter((r) => r.value > 0)
    .sort((x, y) => y.value - x.value)
    .slice(0, 10);

  return (
    <div className="mx-auto flex max-w-[1500px] flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="flex items-center gap-2 text-[18px] font-semibold tracking-tight text-ink">
          <Sparkles size={18} className="accent-text" /> {t("nav.ai")}
          {loading && <RefreshCw size={13} className="animate-spin text-ink-3" />}
        </h1>
        <div className="flex flex-wrap items-center gap-2">
          <Segmented value={range} onChange={setRange} options={RANGES.map((k) => ({ key: k, label: t(`ai.range.${k}` as never) }))} size="md" />
          {range === "custom" && (
            <div className="flex items-center gap-1.5">
              <TextInput type="date" className="w-[140px]" value={custom.from} max={custom.to} onChange={(e) => e.target.value && setCustom((c) => ({ ...c, from: e.target.value }))} />
              <span className="text-ink-3">→</span>
              <TextInput type="date" className="w-[140px]" value={custom.to} min={custom.from} onChange={(e) => e.target.value && setCustom((c) => ({ ...c, to: e.target.value }))} />
            </div>
          )}
          <Button onClick={doImport} loading={importing}>
            <FileUp size={13} /> {t("ai.import")}
          </Button>
          <Button onClick={syncAll} loading={syncingAll}>
            <RefreshCw size={13} /> {t("ai.syncAll")}
          </Button>
        </div>
      </div>

      {error && <Notice tone="error">{error}</Notice>}
      {syncReport === "none" && (
        <Notice tone="warn">
          {t("ai.sync.none")}{" "}
          <button className="accent-text underline underline-offset-2" onClick={() => onOpenSettings()}>
            {t("ai.manage")}
          </button>
        </Notice>
      )}
      {Array.isArray(syncReport) && (
        <Notice tone={syncReport.every((r) => r.ok) ? "good" : "warn"}>
          <div className="font-medium">{t("ai.sync.done")}</div>
          {syncReport.map((r) => (
            <div key={r.provider}>
              <span style={{ color: r.ok ? STATUS.good : STATUS.critical }}>{r.ok ? "✓" : "✕"}</span> {PROVIDER_LABEL[r.provider]}: {r.text}
            </div>
          ))}
        </Notice>
      )}
      {report && (
        <Notice tone={report.errors.length ? "warn" : "good"}>
          {t("ai.imported", { n: report.imported, m: report.totalRows })}
          {report.errors.slice(0, 5).map((e) => (
            <div key={e.row}>
              Row {e.row}: {e.message}
            </div>
          ))}
          {report.errors.length > 5 && <div>…and {report.errors.length - 5} more errors.</div>}
        </Notice>
      )}

      {showOnboarding && <Onboarding t={t} onSettings={() => onOpenSettings()} onImport={doImport} />}

      {/* Active filter */}
      <div className="flex min-h-[26px] flex-wrap items-center gap-2 text-[11.5px]">
        <Filter size={13} className={filtering ? "accent-text" : "text-ink-3"} />
        {filtering ? (
          <>
            {filter.providers.map((p) => (
              <button
                key={p}
                onClick={() => toggleProvider(p)}
                className="accent-ring inline-flex items-center gap-1.5 rounded-full border border-line-strong bg-surface-2 px-2 py-0.5 text-ink hover:bg-surface-3"
              >
                <span className="h-2 w-2 rounded-sm" style={{ background: PROVIDER_COLORS[p] }} />
                {PROVIDER_LABEL[p]}
                <X size={11} className="text-ink-3" />
              </button>
            ))}
            {filter.models.map((m) => (
              <button
                key={m}
                onClick={() => toggleModel(m)}
                className="accent-ring inline-flex items-center gap-1.5 rounded-full border border-line-strong bg-surface-2 px-2 py-0.5 font-mono text-ink hover:bg-surface-3"
              >
                {m}
                <X size={11} className="text-ink-3" />
              </button>
            ))}
            <Button variant="ghost" onClick={() => setFilter(NO_FILTER)}>
              {t("ai.filter.clear")}
            </Button>
          </>
        ) : (
          <span className="text-ink-3">{t("ai.filter.hint")}</span>
        )}
      </div>

      <div className="grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-6">
        <Kpi label={t("ai.kpi.total")} value={fmtTokens(totals?.total)} sub={t("ai.kpi.requests", { n: fmtInt(totals?.requests) })} />
        <Kpi label={t("ai.kpi.input")} value={fmtTokens(totals ? totalInput(totals) : null)} sub={t("ai.kpi.inclCached", { n: fmtTokens(totals?.cached) })} />
        <Kpi label={t("ai.kpi.output")} value={fmtTokens(totals?.output)} sub={totals?.reasoning ? t("ai.kpi.reasoning", { n: fmtTokens(totals.reasoning) }) : undefined} />
        <Kpi label={t("ai.kpi.actual")} value={totals && totals.actualCost > 0 ? fmtUsd(totals.actualCost) : "N/A"} sub={t("ai.kpi.actualSub")} />
        <Kpi
          label={t("ai.kpi.estimated")}
          value={totals && totals.estimatedCost > 0 ? fmtUsd(totals.estimatedCost) : "—"}
          sub={totals && totals.unpricedTokens > 0 ? t("ai.kpi.unpriced", { n: fmtTokens(totals.unpricedTokens) }) : t("ai.kpi.estimatedSub")}
        />
        <Kpi
          label={t("ai.kpi.dedup")}
          value={fmtInt(a?.deduplicatedRecords)}
          sub={t("ai.kpi.dedupSub", { n: fmtInt(a?.recordCount) })}
          title="provider_reported > imported > telemetry, per provider and UTC day"
        />
      </div>

      <div>
        <div className="mb-2 flex items-center justify-between">
          <h2 className="text-[13px] font-medium uppercase tracking-wider text-ink-3">{t("section.providers")}</h2>
          <Button variant="ghost" onClick={() => onOpenSettings()}>
            <Settings2 size={13} /> {t("ai.manage")}
          </Button>
        </div>
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
          {providers.map((p) => (
            <ProviderCard
              key={p.id}
              p={p}
              b={byProvider.get(p.id)}
              selected={filter.providers.includes(p.id)}
              dimmed={filter.providers.length > 0 && !filter.providers.includes(p.id)}
              onSelect={() => toggleProvider(p.id)}
              onConfigure={() => onOpenSettings(p.id)}
            />
          ))}
        </div>
      </div>

      <h2 className="mt-2 text-[13px] font-medium uppercase tracking-wider text-ink-3">{t("section.analytics")}</h2>
      {a && (
        <>
          <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
            <Card
              title={spanDays > 62 ? t("ai.chart.monthlyTokens") : t("ai.chart.dailyTokens")}
              icon={<BarChart3 size={15} />}
              right={
                <Segmented
                  value={metric}
                  onChange={setMetric}
                  options={[
                    { key: "tokens", label: t("ai.chart.tokens") },
                    { key: "cost", label: t("ai.chart.cost") },
                  ]}
                />
              }
            >
              <UsageOverTime
                a={metric === "tokens" && filter.models.length === 0 && all ? all : a}
                monthly={spanDays > 62}
                metric={metric}
                selected={filter.providers}
                onToggleProvider={toggleProvider}
                t={t}
              />
            </Card>
            <Card title={t("ai.chart.io")} icon={<BarChart3 size={15} />}>
              <InputOutput a={a} t={t} />
            </Card>
          </div>
          <div className="grid grid-cols-1 gap-4 xl:grid-cols-3">
            <Card title={t("ai.chart.models")} icon={<BarChart3 size={15} />}>
              <HBarChart rows={modelRows} valueFmt={fmtTokens} emptyLabel={t("ai.chart.empty")} active={filter.models} onSelect={(r) => toggleModel(r.id)} />
            </Card>
            <Card title={t("ai.chart.costProvider")} icon={<Coins size={15} />} footer={t("ai.chart.costFooter")}>
              <HBarChart
                rows={costProviderRows}
                valueFmt={fmtUsd}
                emptyLabel={t("ai.chart.empty")}
                active={filter.providers}
                onSelect={(r) => toggleProvider(r.id as ProviderId)}
              />
            </Card>
            <Card title={t("ai.chart.costModel")} icon={<Coins size={15} />}>
              <HBarChart rows={costModelRows} valueFmt={fmtUsd} emptyLabel={t("ai.chart.empty")} active={filter.models} onSelect={(r) => toggleModel(r.id)} />
            </Card>
          </div>
          {a.monthly.length > 1 && spanDays <= 62 && (
            <Card title={t("ai.chart.monthlyTokens")} icon={<BarChart3 size={15} />}>
              <UsageOverTime a={filter.models.length === 0 && all ? all : a} monthly metric="tokens" selected={filter.providers} onToggleProvider={toggleProvider} t={t} />
            </Card>
          )}
        </>
      )}

      <Card title={t("ai.telemetry.title")} icon={<Sparkles size={15} />} footer={t("ai.telemetry.footer")}>
        <TelemetryLog version={usageVersion} filter={filter} t={t} />
      </Card>

      <Notice>{t("ai.privacy")}</Notice>
    </div>
  );
}
