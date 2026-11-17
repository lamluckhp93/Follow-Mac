import { getCurrentWindow } from "@tauri-apps/api/window";
import { Battery, BatteryCharging, LayoutDashboard, Maximize2, Minimize2, X } from "lucide-react";
import { Sparkline } from "../components/charts";
import { PROVIDER_COLORS, SERIES } from "../lib/colors";
import { fmtPct, fmtRate, fmtTokens, fmtUsd, NA } from "../lib/format";
import { api } from "../lib/api";
import { useAi } from "../store/ai";
import { useSettings } from "../store/settings";
import { useSystem } from "../store/system";

function Stat({ label, value, sub }: { label: string; value: string; sub?: React.ReactNode }) {
  return (
    <div className="min-w-0" data-tauri-drag-region>
      <div className="text-[9.5px] uppercase tracking-[0.14em] text-ink-3" data-tauri-drag-region>
        {label}
      </div>
      <div className="font-mono text-[17px] font-semibold leading-tight text-ink" data-tauri-drag-region>
        {value}
      </div>
      {sub && (
        <div className="truncate text-[10px] text-ink-3" data-tauri-drag-region>
          {sub}
        </div>
      )}
    </div>
  );
}

export default function Widget() {
  const snap = useSystem((s) => s.snapshot);
  const live = useSystem((s) => s.live);
  const summary = useAi((s) => s.summary);
  const providers = useAi((s) => s.providers);
  const settings = useSettings((s) => s.settings);
  const update = useSettings((s) => s.update);
  const mode = settings?.widget.mode ?? "compact";
  const opacity = settings?.widget.opacity ?? 0.92;
  const expanded = mode === "expanded";

  const b = snap?.battery;
  const batteryValue = !b ? NA : b.status === "not_present" ? "—" : fmtPct(b.percent);
  const cost = summary ? summary.today.actualCost + summary.today.estimatedCost : null;

  return (
    <div
      data-tauri-drag-region
      className="group flex h-full w-full flex-col overflow-hidden rounded-2xl border border-line-strong p-3 text-ink"
      style={{
        background: `linear-gradient(160deg, rgba(20,27,48,${opacity}), rgba(7,10,20,${opacity}))`,
        backdropFilter: "blur(18px)",
        WebkitBackdropFilter: "blur(18px)",
      }}
    >
      <div className="mb-2 flex items-center justify-between" data-tauri-drag-region>
        <div className="flex items-center gap-1.5 text-[10px] font-semibold tracking-[0.2em] text-ink-2" data-tauri-drag-region>
          <span
            className={snap?.monitoring ? "pulse-dot h-1.5 w-1.5 rounded-full" : "h-1.5 w-1.5 rounded-full"}
            style={{ background: snap?.monitoring ? "#0ca30c" : "#6b7591" }}
          />
          MAC OBSERVER
        </div>
        <div className="flex items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
          <button className="rounded p-1 text-ink-3 hover:bg-surface-3 hover:text-ink" title="Open dashboard" onClick={() => api.showMainWindow()}>
            <LayoutDashboard size={12} />
          </button>
          <button
            className="rounded p-1 text-ink-3 hover:bg-surface-3 hover:text-ink"
            title={expanded ? "Compact" : "Expand"}
            onClick={() => api.setWidgetMode(expanded ? "compact" : "expanded")}
          >
            {expanded ? <Minimize2 size={12} /> : <Maximize2 size={12} />}
          </button>
          <button
            className="rounded p-1 text-ink-3 hover:bg-surface-3 hover:text-ink"
            title="Hide widget"
            onClick={async () => {
              // The backend closes this window when the setting is saved; close locally as a fallback.
              const err = await update((s) => void (s.widget.enabled = false));
              if (err) await getCurrentWindow().close();
            }}
          >
            <X size={12} />
          </button>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-x-4 gap-y-2" data-tauri-drag-region>
        <Stat label="CPU" value={fmtPct(snap?.cpu.usage)} />
        <Stat label="RAM" value={fmtPct(snap?.memory.usedPercent)} sub={snap?.memory.pressureLabel && snap.memory.pressureLabel !== "normal" ? `pressure ${snap.memory.pressureLabel}` : undefined} />
        <Stat
          label="Battery"
          value={batteryValue}
          sub={
            b?.charging ? (
              <span className="inline-flex items-center gap-0.5">
                <BatteryCharging size={10} /> charging
              </span>
            ) : b?.status === "ok" ? (
              <span className="inline-flex items-center gap-0.5">
                <Battery size={10} /> on battery
              </span>
            ) : undefined
          }
        />
        <Stat label="AI today" value={fmtTokens(summary?.today.total ?? null)} sub={cost != null && cost > 0 ? `${summary!.today.estimatedCost > 0 ? "~" : ""}${fmtUsd(cost)}` : "tokens"} />
      </div>

      {expanded && (
        <div className="mt-3 flex flex-1 flex-col gap-3" data-tauri-drag-region>
          <div>
            <div className="mb-0.5 flex justify-between text-[10px] text-ink-3">
              <span>CPU · 5 min</span>
              <span className="font-mono">{fmtPct(snap?.cpu.usage)}</span>
            </div>
            <Sparkline data={live} dataKey="cpu" height={40} domain={[0, 100]} />
          </div>
          <div>
            <div className="mb-0.5 flex justify-between text-[10px] text-ink-3">
              <span>RAM · 5 min</span>
              <span className="font-mono">{fmtPct(snap?.memory.usedPercent)}</span>
            </div>
            <Sparkline data={live} dataKey="mem" height={40} domain={[0, 100]} color={SERIES.b} />
          </div>
          <div className="grid grid-cols-2 gap-2 text-[11px]">
            <div>
              <div className="text-[9.5px] uppercase tracking-wider text-ink-3">↓ Download</div>
              <div className="font-mono text-ink">{fmtRate(snap?.network.rxBps)}</div>
            </div>
            <div>
              <div className="text-[9.5px] uppercase tracking-wider text-ink-3">↑ Upload</div>
              <div className="font-mono text-ink">{fmtRate(snap?.network.txBps)}</div>
            </div>
          </div>
          <div className="border-t border-line pt-2">
            <div className="mb-1 flex justify-between text-[9.5px] uppercase tracking-wider text-ink-3">
              <span>Providers</span>
              <span>month {fmtTokens(summary?.month.total ?? null)}</span>
            </div>
            {providers.filter((p) => p.status !== "not_configured").length === 0 ? (
              <div className="text-[11px] text-ink-3">No providers configured</div>
            ) : (
              providers
                .filter((p) => p.status !== "not_configured")
                .map((p) => (
                  <div key={p.id} className="flex items-center justify-between py-[2px] text-[11px]">
                    <span className="flex items-center gap-1.5 text-ink-2">
                      <span className="h-2 w-2 rounded-sm" style={{ background: PROVIDER_COLORS[p.id] }} />
                      {p.displayName}
                    </span>
                    <span className="text-ink-3">{p.status.replace("_", " ")}</span>
                  </div>
                ))
            )}
          </div>
        </div>
      )}
    </div>
  );
}
