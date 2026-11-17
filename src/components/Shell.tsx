import { Activity, Bell, Cpu, PauseCircle, PlayCircle, Settings as SettingsIcon, Sparkles } from "lucide-react";
import { useEffect, useState } from "react";
import { fmtTokens } from "../lib/format";
import { useT } from "../lib/i18n";
import { useAi } from "../store/ai";
import { useSettings } from "../store/settings";
import { useSystem } from "../store/system";
import { cx } from "./ui";

export type View = "dashboard" | "ai" | "alerts" | "settings";

function Logo() {
  return (
    <div className="flex items-center gap-2.5">
      <svg width="26" height="26" viewBox="0 0 26 26" aria-hidden>
        <defs>
          <linearGradient id="lg" x1="0" y1="1" x2="1" y2="0">
            <stop offset="0" stopColor="#a855f7" />
            <stop offset="0.5" stopColor="#3b82f6" />
            <stop offset="1" stopColor="#22d3ee" />
          </linearGradient>
        </defs>
        <path d="M6.2 20.5 A9.5 9.5 0 1 1 19.8 20.5" fill="none" stroke="url(#lg)" strokeWidth="3" strokeLinecap="round" />
        <circle cx="13" cy="13" r="3.4" fill="#22d3ee" />
      </svg>
      <div className="leading-none">
        <div className="text-[13px] font-semibold tracking-[0.18em] text-ink">MAC OBSERVER</div>
        <div className="mt-1 text-[10px] tracking-wider text-ink-3">SYSTEM · AI USAGE</div>
      </div>
    </div>
  );
}

function Clock() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(id);
  }, []);
  return (
    <div className="text-right leading-tight">
      <div className="font-mono text-[15px] text-ink">{now.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", second: "2-digit" })}</div>
      <div className="text-[10.5px] text-ink-3">{now.toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" })}</div>
    </div>
  );
}

export function Header({ onSettings }: { onSettings: () => void }) {
  const t = useT();
  const device = useSystem((s) => s.device);
  const snap = useSystem((s) => s.snapshot);
  const settings = useSettings((s) => s.settings);
  const update = useSettings((s) => s.update);
  const monitoring = settings?.monitoring.enabled ?? true;
  const fresh = snap && Date.now() - snap.ts < 10_000;

  return (
    <header data-tauri-drag-region className="flex h-[68px] shrink-0 items-center justify-between gap-6 border-b border-line pl-[92px] pr-5">
      <Logo />
      <div className="flex min-w-0 flex-1 items-center gap-5 text-[12px] text-ink-2" data-tauri-drag-region>
        <div className="min-w-0 border-l border-line pl-5" data-tauri-drag-region>
          <div className="truncate font-medium text-ink">{device?.deviceName ?? "—"}</div>
          <div className="truncate text-[11px] text-ink-3">
            {[device?.chip, device?.osName && device.osVersion ? `${device.osName} ${device.osVersion}` : null, device?.osBuild]
              .filter(Boolean)
              .join(" · ") || "—"}
          </div>
        </div>
      </div>
      <div className="flex items-center gap-4" style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}>
        <button
          onClick={() => update((s) => void (s.monitoring.enabled = !s.monitoring.enabled))}
          className="accent-ring flex items-center gap-2 rounded-full border border-line-strong bg-surface-2/80 px-3 py-1 text-[11.5px] text-ink hover:bg-surface-3"
          title={monitoring ? "Pause monitoring" : "Resume monitoring"}
        >
          {monitoring ? (
            <>
              <span className={cx("h-2 w-2 rounded-full", fresh ? "pulse-dot" : "")} style={{ background: fresh ? "#0ca30c" : "#fab219" }} />
              {fresh ? t("status.live") : "Waiting"}
              <PauseCircle size={13} className="text-ink-3" />
            </>
          ) : (
            <>
              <span className="h-2 w-2 rounded-full bg-[#6b7591]" />
              {t("status.paused")}
              <PlayCircle size={13} className="text-ink-3" />
            </>
          )}
        </button>
        <Clock />
        <button onClick={onSettings} className="accent-ring rounded-lg p-2 text-ink-2 hover:bg-surface-2 hover:text-ink" title="Settings" aria-label="Settings">
          <SettingsIcon size={17} />
        </button>
      </div>
    </header>
  );
}

export function Sidebar({ view, onChange }: { view: View; onChange: (v: View) => void }) {
  const t = useT();
  const summary = useAi((s) => s.summary);
  const items: { key: View; label: string; icon: React.ReactNode; meta?: string }[] = [
    { key: "dashboard", label: t("nav.overview"), icon: <Cpu size={17} /> },
    { key: "ai", label: t("nav.ai"), icon: <Sparkles size={17} />, meta: summary ? fmtTokens(summary.today.total) : undefined },
    { key: "alerts", label: t("nav.alerts"), icon: <Bell size={17} /> },
    { key: "settings", label: t("nav.settings"), icon: <SettingsIcon size={17} /> },
  ];
  return (
    <nav className="flex w-[196px] shrink-0 flex-col gap-1 border-r border-line px-3 py-4">
      {items.map((i) => (
        <button
          key={i.key}
          onClick={() => onChange(i.key)}
          className={cx(
            "accent-ring group relative flex items-center gap-3 rounded-xl px-3 py-2 text-[13px] transition-colors",
            view === i.key ? "bg-surface-2 text-ink" : "text-ink-2 hover:bg-surface-2/60 hover:text-ink",
          )}
        >
          {view === i.key && <span className="accent-bg absolute left-0 top-2 bottom-2 w-[3px] rounded-full" />}
          <span className={view === i.key ? "accent-text" : "text-ink-3 group-hover:text-ink-2"}>{i.icon}</span>
          <span className="flex-1 text-left">{i.label}</span>
          {i.meta && <span className="font-mono text-[10.5px] text-ink-3" title="Tokens today">{i.meta}</span>}
        </button>
      ))}
      <div className="mt-auto flex items-center gap-2 px-3 text-[10.5px] text-ink-3">
        <Activity size={12} /> Real data only · no mocks
      </div>
    </nav>
  );
}
