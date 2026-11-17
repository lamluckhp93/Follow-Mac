import { ask, save } from "@tauri-apps/plugin-dialog";
import { Archive, Database, Download, Info, Settings as Cog } from "lucide-react";
import { useEffect, useState } from "react";
import { AlertRulesEditor } from "../components/AlertRules";
import { Button, Card, KV, Notice, NumberField, Row, Segmented, Toggle, cx } from "../components/ui";
import { api, errorMessage } from "../lib/api";
import { fmtBytes, fmtDateTime, fmtInt } from "../lib/format";
import type { AppInfo, Settings, StorageStats } from "../lib/types";
import { useSettings } from "../store/settings";
import { useSystem } from "../store/system";
import { ProviderSettings } from "./ProviderSettings";

type Tab = "general" | "appearance" | "monitoring" | "ai" | "alerts" | "history" | "about";
const TABS: { key: Tab; label: string }[] = [
  { key: "general", label: "General" },
  { key: "appearance", label: "Appearance" },
  { key: "monitoring", label: "System Monitoring" },
  { key: "ai", label: "AI Providers" },
  { key: "alerts", label: "Alerts" },
  { key: "history", label: "History & Storage" },
  { key: "about", label: "About" },
];

function useUpdate() {
  const update = useSettings((s) => s.update);
  const [err, setErr] = useState<string | null>(null);
  const run = async (fn: (s: Settings) => void) => setErr(await update(fn));
  return { run, err };
}

function General({ s }: { s: Settings }) {
  const { run, err } = useUpdate();
  return (
    <Card title="General">
      {err && <Notice tone="error">{err}</Notice>}
      <Row label="Launch at login" hint="Registers a macOS LaunchAgent. The app starts hidden in the menu bar.">
        <Toggle checked={s.general.launchAtLogin} onChange={(v) => run((x) => void (x.general.launchAtLogin = v))} />
      </Row>
      <Row label="Start minimized" hint="Open to the menu bar only; the dashboard window stays hidden until you open it.">
        <Toggle checked={s.general.startMinimized} onChange={(v) => run((x) => void (x.general.startMinimized = v))} />
      </Row>
      <Row label="Monitoring" hint="Pause or resume all system sampling.">
        <Toggle checked={s.monitoring.enabled} onChange={(v) => run((x) => void (x.monitoring.enabled = v))} />
      </Row>
      <Row label="Default view">
        <Segmented
          value={s.general.defaultView}
          onChange={(v) => run((x) => void (x.general.defaultView = v))}
          options={[
            { key: "dashboard", label: "Overview" },
            { key: "ai", label: "AI Usage" },
            { key: "alerts", label: "Alerts" },
          ]}
        />
      </Row>
      <Row label="Language" hint="Vietnamese covers navigation and key labels; the rest falls back to English.">
        <Segmented
          value={s.general.language}
          onChange={(v) => run((x) => void (x.general.language = v))}
          options={[
            { key: "en", label: "English" },
            { key: "vi", label: "Tiếng Việt" },
          ]}
        />
      </Row>
    </Card>
  );
}

function Appearance({ s }: { s: Settings }) {
  const { run, err } = useUpdate();
  return (
    <div className="flex flex-col gap-4">
      <Card title="Appearance">
        {err && <Notice tone="error">{err}</Notice>}
        <Row label="Theme" hint="Mac Observer is designed dark-first.">
          <Segmented value="dark" onChange={() => {}} options={[{ key: "dark", label: "Dark" }]} />
        </Row>
        <Row label="Accent color">
          <div className="flex gap-2">
            {(["cyan", "blue", "purple"] as const).map((a) => (
              <button
                key={a}
                onClick={() => run((x) => void (x.appearance.accent = a))}
                aria-label={a}
                className={cx("accent-ring h-6 w-6 rounded-full border-2", s.appearance.accent === a ? "border-white" : "border-transparent")}
                style={{ background: { cyan: "#22d3ee", blue: "#3b82f6", purple: "#a855f7" }[a] }}
              />
            ))}
          </div>
        </Row>
        <Row label="Compact mode" hint="Tighter card padding.">
          <Toggle checked={s.appearance.compact} onChange={(v) => run((x) => void (x.appearance.compact = v))} />
        </Row>
        <Row label="Animations" hint="Gauge transitions and fade-ins. macOS “Reduce motion” is always respected.">
          <Toggle checked={s.appearance.animations} onChange={(v) => run((x) => void (x.appearance.animations = v))} />
        </Row>
      </Card>
      <Card title="Menu bar">
        <Row label="Show in menu bar title" hint="The dropdown always shows full details.">
          <Toggle checked={s.menuBar.enabled} onChange={(v) => run((x) => void (x.menuBar.enabled = v))} />
        </Row>
        {(
          [
            ["showCpu", "CPU percentage"],
            ["showRam", "RAM percentage"],
            ["showTokens", "AI tokens today"],
            ["showStatus", "Monitoring status (⏸ when paused)"],
          ] as const
        ).map(([k, label]) => (
          <Row key={k} label={label}>
            <Toggle disabled={!s.menuBar.enabled} checked={s.menuBar[k]} onChange={(v) => run((x) => void (x.menuBar[k] = v))} />
          </Row>
        ))}
      </Card>
      <Card title="Floating widget">
        <Row label="Show floating widget" hint="Always-on-top, frameless; drag it anywhere and resize from the edges.">
          <Toggle checked={s.widget.enabled} onChange={(v) => run((x) => void (x.widget.enabled = v))} />
        </Row>
        <Row label="Mode">
          <Segmented
            value={s.widget.mode}
            onChange={(v) => run((x) => void (x.widget.mode = v))}
            options={[
              { key: "compact", label: "Compact" },
              { key: "expanded", label: "Expanded" },
            ]}
          />
        </Row>
        <Row label="Opacity">
          <input
            type="range"
            min={0.5}
            max={1}
            step={0.02}
            value={s.widget.opacity}
            onChange={(e) => run((x) => void (x.widget.opacity = Number(e.target.value)))}
            className="w-40 accent-[var(--accent)]"
          />
        </Row>
      </Card>
    </div>
  );
}

function Monitoring({ s }: { s: Settings }) {
  const [m, setM] = useState(s.monitoring);
  const { run, err } = useUpdate();
  useEffect(() => setM(s.monitoring), [s.monitoring]);
  const dirty = JSON.stringify(m) !== JSON.stringify(s.monitoring);
  const fields: [keyof Settings["monitoring"], string, string, number, number][] = [
    ["cpuIntervalMs", "CPU & GPU", "ms", 500, 60000],
    ["memoryIntervalMs", "Memory", "ms", 500, 60000],
    ["networkIntervalMs", "Network", "ms", 500, 60000],
    ["diskIntervalMs", "Disk I/O", "ms", 1000, 120000],
    ["batteryIntervalMs", "Battery", "ms", 5000, 600000],
    ["sensorsIntervalMs", "Temperature sensors", "ms", 1000, 120000],
    ["processIntervalMs", "Top processes", "ms", 2000, 120000],
    ["persistIntervalS", "History resolution", "s", 5, 300],
  ];
  return (
    <Card title="Polling intervals" footer="Shorter intervals give smoother charts at slightly higher CPU cost. History rows are averaged over the resolution window and written to SQLite in batches every 60 s.">
      {err && <Notice tone="error">{err}</Notice>}
      {fields.map(([k, label, unit, min, max]) => (
        <Row key={k} label={label} hint={`${min}–${max} ${unit}`}>
          <NumberField value={m[k] as number} min={min} max={max} step={unit === "ms" ? 500 : 5} suffix={unit} onChange={(v) => setM({ ...m, [k]: v })} />
        </Row>
      ))}
      <div className="mt-3 flex gap-2">
        <Button variant="primary" disabled={!dirty} onClick={() => run((x) => void (x.monitoring = { ...m, enabled: x.monitoring.enabled }))}>
          Apply
        </Button>
        <Button disabled={!dirty} onClick={() => setM(s.monitoring)}>
          Revert
        </Button>
      </div>
    </Card>
  );
}

function History({ s }: { s: Settings }) {
  const { run, err } = useUpdate();
  const [stats, setStats] = useState<StorageStats | null>(null);
  const [msg, setMsg] = useState<{ tone: "good" | "error"; text: string } | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const load = () => api.storageStats().then(setStats).catch((e) => setMsg({ tone: "error", text: errorMessage(e) }));
  useEffect(() => {
    void load();
  }, []);

  const act = async (key: string, fn: () => Promise<string | null>) => {
    setBusy(key);
    setMsg(null);
    try {
      const text = await fn();
      if (text) setMsg({ tone: "good", text });
      void load();
    } catch (e) {
      setMsg({ tone: "error", text: errorMessage(e) });
    } finally {
      setBusy(null);
    }
  };
  const stamp = new Date().toISOString().slice(0, 10);

  return (
    <div className="flex flex-col gap-4">
      <Card title="Retention" footer="Old rows are deleted hourly. 0 = keep forever.">
        {err && <Notice tone="error">{err}</Notice>}
        <Row label="System metrics" hint="Downsampled rows (default 10 s resolution).">
          <NumberField value={s.history.systemRetentionDays} min={1} max={3650} suffix="days" onChange={(v) => run((x) => void (x.history.systemRetentionDays = Math.max(1, Math.round(v))))} />
        </Row>
        <Row label="AI usage & cost records">
          <NumberField value={s.history.aiRetentionDays} min={0} max={3650} suffix="days" onChange={(v) => run((x) => void (x.history.aiRetentionDays = Math.max(0, Math.round(v))))} />
        </Row>
        <Row label="Alert history">
          <NumberField value={s.history.alertRetentionDays} min={0} max={3650} suffix="days" onChange={(v) => run((x) => void (x.history.alertRetentionDays = Math.max(0, Math.round(v))))} />
        </Row>
        <div className="mt-2">
          <Button loading={busy === "ret"} onClick={() => act("ret", async () => `Removed ${await api.runRetention()} rows.`)}>
            Apply retention now
          </Button>
        </div>
      </Card>
      <Card title="Database" icon={<Database size={15} />}>
        {stats && (
          <>
            <KV k="Size on disk" v={fmtBytes(stats.dbBytes)} />
            <KV k="System metric rows" v={fmtInt(stats.systemMetricsRows)} />
            <KV k="Oldest metric" v={fmtDateTime(stats.oldestMetricTs)} />
            <KV k="AI usage rows" v={fmtInt(stats.usageRows)} />
            <KV k="AI cost rows" v={fmtInt(stats.costRows)} />
            <KV k="Alert events" v={fmtInt(stats.alertEventRows)} />
            <KV k="Schema version" v={stats.schemaVersion} />
          </>
        )}
        {msg && (
          <div className="mt-2">
            <Notice tone={msg.tone}>{msg.text}</Notice>
          </div>
        )}
        <div className="mt-3 flex flex-wrap gap-2">
          <Button
            loading={busy === "backup"}
            onClick={() =>
              act("backup", async () => {
                const p = await save({ defaultPath: `mac-observer-backup-${stamp}.db`, filters: [{ name: "SQLite", extensions: ["db"] }] });
                return p ? `Backup written to ${await api.backupDatabase(p)}` : null;
              })
            }
          >
            <Archive size={13} /> Backup database
          </Button>
          <Button
            loading={busy === "sys"}
            onClick={() =>
              act("sys", async () => {
                const p = await save({ defaultPath: `system-metrics-${stamp}.csv`, filters: [{ name: "CSV", extensions: ["csv"] }] });
                return p ? `Exported ${await api.exportSystemCsv(p, Math.max(1, s.history.systemRetentionDays))} rows.` : null;
              })
            }
          >
            <Download size={13} /> Export system metrics (CSV)
          </Button>
          <Button
            loading={busy === "ai"}
            onClick={() =>
              act("ai", async () => {
                const p = await save({ defaultPath: `ai-usage-${stamp}.csv`, filters: [{ name: "CSV", extensions: ["csv"] }] });
                return p ? `Exported ${await api.exportAiCsv(p)} usage records (re-importable).` : null;
              })
            }
          >
            <Download size={13} /> Export AI usage (CSV)
          </Button>
        </div>
      </Card>
      <Card title="Delete data" footer="These actions cannot be undone. Make a backup first.">
        <div className="flex flex-wrap gap-2">
          {(
            [
              ["telemetry", "Delete telemetry records"],
              ["imported", "Delete imported records"],
              ["provider_reported", "Delete provider-reported records (re-sync restores them)"],
            ] as const
          ).map(([src, label]) => (
            <Button
              key={src}
              variant="danger"
              loading={busy === src}
              onClick={() =>
                act(src, async () => {
                  if (!(await ask(`${label}? This cannot be undone.`, { title: "Delete data", kind: "warning" }))) return null;
                  return `Deleted ${await api.deleteUsageSource(src)} rows.`;
                })
              }
            >
              {label}
            </Button>
          ))}
        </div>
      </Card>
    </div>
  );
}

function About() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const device = useSystem((s) => s.device);
  useEffect(() => {
    api.appInfo().then(setInfo).catch(() => setInfo(null));
  }, []);
  return (
    <div className="flex flex-col gap-4">
      <Card title="Mac Observer" icon={<Info size={15} />}>
        <KV k="Version" v={info?.version ?? "…"} />
        <KV k="Tauri" v={info?.tauriVersion ?? "…"} />
        <KV k="Launch at login (system)" v={info?.autostartEnabled == null ? "unknown" : info.autostartEnabled ? "enabled" : "disabled"} />
        <KV k="Database" v={<span className="selectable">{info?.databasePath}</span>} />
        <KV k="Logs" v={<span className="selectable">{info?.logDir}</span>} />
      </Card>
      <Card title="This Mac">
        <KV k="Name" v={device?.deviceName ?? "N/A"} />
        <KV k="Model identifier" v={device?.modelIdentifier ?? "N/A"} />
        <KV k="Chip" v={device?.chip ?? "N/A"} />
        <KV k="macOS" v={`${device?.osVersion ?? "?"} (${device?.osBuild ?? "?"})`} />
        <KV k="Kernel" v={device?.kernelVersion ?? "N/A"} />
        <KV k="Memory" v={fmtBytes(device?.totalMemory, 0)} />
      </Card>
      <Card title="Data sources & privacy">
        <div className="selectable flex flex-col gap-2 text-[12px] leading-relaxed text-ink-2">
          <p>
            <b className="text-ink">System metrics</b> come from macOS kernel and IOKit interfaces (host_statistics, sysctl, IOAccelerator,
            AppleSmartBattery, IOBlockStorageDriver, IOHID sensors). Nothing is simulated; values that cannot be read show N/A, Unsupported or Not
            present.
          </p>
          <p>
            <b className="text-ink">AI usage</b> comes only from official provider APIs, from requests that pass through Mac Observer, or from
            files you import. Mac Observer does not read other apps' or browsers' data.
          </p>
          <p>
            <b className="text-ink">Secrets</b> live in the macOS Keychain only. Prompt and response content is never stored.
          </p>
        </div>
      </Card>
    </div>
  );
}

export default function SettingsPage({ initialTab, initialProvider }: { initialTab?: string; initialProvider?: string }) {
  const s = useSettings((x) => x.settings);
  const error = useSettings((x) => x.error);
  const [tab, setTab] = useState<Tab>((TABS.find((t) => t.key === initialTab)?.key as Tab) ?? "general");
  return (
    <div className="mx-auto flex max-w-[1200px] flex-col gap-4">
      <h1 className="flex items-center gap-2 text-[18px] font-semibold tracking-tight text-ink">
        <Cog size={18} className="accent-text" /> Settings
      </h1>
      <div className="flex flex-wrap gap-1 border-b border-line pb-2">
        {TABS.map((t) => (
          <button
            key={t.key}
            onClick={() => setTab(t.key)}
            className={cx(
              "accent-ring rounded-lg px-3 py-1.5 text-[12.5px]",
              tab === t.key ? "bg-surface-2 text-ink" : "text-ink-3 hover:text-ink-2",
            )}
          >
            {t.label}
          </button>
        ))}
      </div>
      {error && <Notice tone="error">{error}</Notice>}
      {s && (
        <>
          {tab === "general" && <General s={s} />}
          {tab === "appearance" && <Appearance s={s} />}
          {tab === "monitoring" && <Monitoring s={s} />}
          {tab === "ai" && <ProviderSettings initialProvider={initialProvider} />}
          {tab === "alerts" && (
            <Card title="Alert rules">
              <AlertRulesEditor />
            </Card>
          )}
          {tab === "history" && <History s={s} />}
          {tab === "about" && <About />}
        </>
      )}
    </div>
  );
}
