import { BatteryCharging, BatteryFull, Cpu, Fan, Gauge, HardDrive, ListOrdered, MemoryStick, Network, Thermometer } from "lucide-react";
import { RingGauge, Sparkline } from "../components/charts";
import { PerformanceChart } from "../components/PerformanceChart";
import { Badge, Bar, Card, Empty, KV, MetricValue, Notice, Updated } from "../components/ui";
import { levelColor, SERIES, STATUS } from "../lib/colors";
import { fmtBytes, fmtDuration, fmtPct, fmtRate, fmtStorage, fmtTemp, fmtUptime, NA } from "../lib/format";
import { useT } from "../lib/i18n";
import type { SystemSnapshot } from "../lib/types";
import { useSystem } from "../store/system";

function pressureTone(level: number | null): "good" | "warn" | "bad" | "neutral" {
  if (level == null) return "neutral";
  return level >= 4 ? "bad" : level >= 2 ? "warn" : "good";
}

function CpuCard({ s }: { s: SystemSnapshot }) {
  const live = useSystem((x) => x.live);
  const device = useSystem((x) => x.device);
  const c = s.cpu;
  return (
    <Card title="CPU" icon={<Cpu size={15} />} right={<Updated ts={c.updatedAt} />}>
      <div className="flex items-center gap-4">
        <RingGauge value={c.usage} label={c.usage != null ? `${c.usage.toFixed(0)}%` : NA} sub="usage" />
        <div className="min-w-0 flex-1">
          <KV k="User" v={fmtPct(c.user, 1)} />
          <KV k="System" v={fmtPct(c.system, 1)} />
          <KV k="Idle" v={fmtPct(c.idle, 1)} />
          <KV k="Load avg" v={c.loadAvg ? c.loadAvg.map((l) => l.toFixed(2)).join(" · ") : NA} />
        </div>
      </div>
      <div className="mt-3 grid gap-[3px]" style={{ gridTemplateColumns: `repeat(${Math.max(c.perCore.length, 1)}, minmax(0, 1fr))` }}>
        {c.perCore.map((v, i) => (
          <div key={i} className="flex h-8 items-end overflow-hidden rounded-[3px] bg-surface-3" title={`Core ${i + 1}: ${v.toFixed(0)}%`}>
            <div className="w-full rounded-[3px] transition-[height] duration-500" style={{ height: `${Math.max(4, v)}%`, background: levelColor(v) }} />
          </div>
        ))}
      </div>
      <div className="mt-1 flex justify-between text-[10.5px] text-ink-3">
        <span>
          {c.perCore.length} cores
          {device?.performanceCores != null && device.efficiencyCores != null && ` · ${device.performanceCores}P + ${device.efficiencyCores}E`}
        </span>
        <span>uptime {fmtUptime(device?.bootTime)}</span>
      </div>
      <div className="mt-2">
        <Sparkline data={live} dataKey="cpu" domain={[0, 100]} />
      </div>
    </Card>
  );
}

function MemoryCard({ s }: { s: SystemSnapshot }) {
  const live = useSystem((x) => x.live);
  const m = s.memory;
  return (
    <Card
      title="Memory"
      icon={<MemoryStick size={15} />}
      right={
        <>
          {m.pressureLabel && (
            <Badge tone={pressureTone(m.pressureLevel)} title="kern.memorystatus_vm_pressure_level">
              pressure {m.pressureLabel}
            </Badge>
          )}
          <Updated ts={m.updatedAt} />
        </>
      }
    >
      <div className="flex items-center gap-4">
        <RingGauge value={m.usedPercent} label={fmtPct(m.usedPercent)} sub="used" warn={80} crit={92} />
        <div className="min-w-0 flex-1">
          <KV k="Used / Total" v={`${fmtBytes(m.used)} / ${fmtBytes(m.total, 0)}`} />
          <KV k="Available" v={fmtBytes(m.available)} />
          <KV k="App · Wired" v={`${fmtBytes(m.app)} · ${fmtBytes(m.wired)}`} />
          <KV k="Compressed" v={fmtBytes(m.compressed)} />
          <KV k="Cached files" v={fmtBytes(m.cachedFiles)} />
        </div>
      </div>
      <div className="mt-3">
        <div className="mb-1 flex justify-between text-[11px] text-ink-3">
          <span>Swap</span>
          <span className="font-mono text-ink-2">
            {fmtBytes(m.swapUsed)} / {fmtBytes(m.swapTotal)}
          </span>
        </div>
        <Bar value={m.swapTotal ? ((m.swapUsed ?? 0) / m.swapTotal) * 100 : 0} color={SERIES.b} />
      </div>
      <div className="mt-2">
        <Sparkline data={live} dataKey="mem" domain={[0, 100]} />
      </div>
      {m.error && <div className="mt-1 text-[10.5px] text-ink-3">{m.error}</div>}
    </Card>
  );
}

function GpuCard({ s }: { s: SystemSnapshot }) {
  const live = useSystem((x) => x.live);
  const g = s.gpu;
  return (
    <Card title="GPU" icon={<Gauge size={15} />} right={<Updated ts={g.updatedAt} />}>
      <div className="flex items-center gap-4">
        <RingGauge value={g.usage} label={g.usage != null ? `${g.usage.toFixed(0)}%` : NA} sub="device" />
        <div className="min-w-0 flex-1">
          <KV k="Model" v={g.model ?? NA} />
          <KV k="Cores" v={g.cores ?? NA} />
          <KV k={g.unifiedMemory ? "Memory in use (unified)" : "Memory in use"} v={fmtBytes(g.memoryUsed)} />
          <KV k="Dedicated VRAM" v={g.unifiedMemory ? "Unified memory" : fmtBytes(g.memoryTotal)} />
        </div>
      </div>
      {g.status !== "ok" && g.status !== "pending" && (
        <div className="mt-2">
          <Notice tone="warn">
            <MetricValue status={g.status} value={null} /> — {g.error}
          </Notice>
        </div>
      )}
      <div className="mt-auto pt-3">
        <Sparkline data={live} dataKey="gpu" domain={[0, 100]} />
      </div>
    </Card>
  );
}

function DiskCard({ s }: { s: SystemSnapshot }) {
  const live = useSystem((x) => x.live);
  const d = s.disk;
  return (
    <Card title="Disk" icon={<HardDrive size={15} />} right={<Updated ts={d.updatedAt} />}>
      <div className="flex items-center gap-4">
        <RingGauge value={d.usedPercent} label={fmtPct(d.usedPercent)} sub="used" warn={85} crit={95} />
        <div className="min-w-0 flex-1">
          <KV k="Volume" v={d.name ?? NA} title={d.mountPoint ?? undefined} />
          <KV k="Total" v={fmtStorage(d.total)} />
          <KV k="Used" v={fmtStorage(d.used)} />
          <KV k="Free" v={fmtStorage(d.free)} />
        </div>
      </div>
      <div className="mt-3 grid grid-cols-2 gap-2">
        <div className="rounded-lg bg-surface-2/70 px-2.5 py-1.5">
          <div className="flex items-center gap-1.5 text-[10.5px] uppercase tracking-wider text-ink-3">
            <span className="h-2 w-2 rounded-sm" style={{ background: SERIES.a }} />
            Read
          </div>
          <MetricValue status={d.ioStatus} value={d.readBps != null ? fmtRate(d.readBps) : null} className="font-mono text-[14px] text-ink" />
        </div>
        <div className="rounded-lg bg-surface-2/70 px-2.5 py-1.5">
          <div className="flex items-center gap-1.5 text-[10.5px] uppercase tracking-wider text-ink-3">
            <span className="h-2 w-2 rounded-sm" style={{ background: SERIES.b }} />
            Write
          </div>
          <MetricValue status={d.ioStatus} value={d.writeBps != null ? fmtRate(d.writeBps) : null} className="font-mono text-[14px] text-ink" />
        </div>
      </div>
      <div className="mt-2 grid grid-cols-2 gap-2">
        <Sparkline data={live} dataKey="read" color={SERIES.a} height={28} />
        <Sparkline data={live} dataKey="write" color={SERIES.b} height={28} />
      </div>
    </Card>
  );
}

function NetworkCard({ s }: { s: SystemSnapshot }) {
  const live = useSystem((x) => x.live);
  const n = s.network;
  return (
    <Card title="Network" icon={<Network size={15} />} right={<Updated ts={n.updatedAt} />}>
      <div className="grid grid-cols-2 gap-3">
        {[
          { label: "Download", v: n.rxBps, total: n.totalRx, color: SERIES.a, key: "rx" },
          { label: "Upload", v: n.txBps, total: n.totalTx, color: SERIES.b, key: "tx" },
        ].map((x) => (
          <div key={x.key}>
            <div className="flex items-center gap-1.5 text-[10.5px] uppercase tracking-wider text-ink-3">
              <span className="h-2 w-2 rounded-sm" style={{ background: x.color }} />
              {x.label}
            </div>
            <MetricValue status={n.status} value={x.v != null ? fmtRate(x.v) : null} className="font-mono text-[20px] font-semibold text-ink" />
            <div className="text-[11px] text-ink-3">total {fmtStorage(x.total)}</div>
            <div className="mt-1">
              <Sparkline data={live} dataKey={x.key} color={x.color} height={34} />
            </div>
          </div>
        ))}
      </div>
      <div className="mt-auto pt-2 text-[11px] text-ink-3">
        Interface {n.primaryInterface ?? NA} · totals since boot, physical interfaces only
      </div>
    </Card>
  );
}

function BatteryCard({ s }: { s: SystemSnapshot }) {
  const b = s.battery;
  if (b.status === "not_present") {
    return (
      <Card title="Battery" icon={<BatteryFull size={15} />}>
        <Empty title="No battery">This Mac has no internal battery.</Empty>
      </Card>
    );
  }
  const state = b.charging ? "Charging" : b.fullyCharged ? "Fully charged" : b.pluggedIn ? "On power adapter" : "On battery";
  return (
    <Card
      title="Battery"
      icon={b.charging ? <BatteryCharging size={15} /> : <BatteryFull size={15} />}
      right={<Updated ts={b.updatedAt} />}
    >
      <div className="flex items-end justify-between">
        <MetricValue status={b.status} value={b.percent != null ? `${b.percent.toFixed(0)}%` : null} className="font-mono text-[30px] font-semibold leading-none text-ink" />
        <Badge tone={b.charging ? "good" : b.percent != null && b.percent < 20 ? "warn" : "neutral"}>{state}</Badge>
      </div>
      <div className="mt-2">
        <Bar value={b.percent} height={8} color={b.percent != null && b.percent < 15 ? STATUS.critical : b.charging ? STATUS.good : "var(--accent)"} />
      </div>
      <div className="mt-3">
        <KV k="Health (capacity)" v={b.healthPercent != null ? `${b.healthPercent.toFixed(0)}%` : NA} title="Full-charge capacity ÷ design capacity" />
        <KV k="Cycle count" v={b.cycleCount ?? NA} />
        <KV k="Capacity" v={b.fullChargeCapacityMah && b.designCapacityMah ? `${b.fullChargeCapacityMah} / ${b.designCapacityMah} mAh` : NA} />
        <KV k={b.charging ? "Time to full" : "Time remaining"} v={b.timeRemainingMin != null ? fmtDuration(b.timeRemainingMin) : "Calculating…"} />
        <KV k="Temperature" v={fmtTemp(b.temperatureC)} />
      </div>
    </Card>
  );
}

function ThermalCard({ s }: { s: SystemSnapshot }) {
  const t = s.temperature;
  const f = s.fan;
  const hottest = [...t.sensors].sort((a, b) => b.celsius - a.celsius).slice(0, 4);
  return (
    <Card title="Thermals & Fan" icon={<Thermometer size={15} />} right={<Updated ts={t.updatedAt} />}>
      <div className="grid grid-cols-3 gap-2">
        {[
          { k: "CPU", v: t.cpuC },
          { k: "GPU", v: t.gpuC },
          { k: "Battery", v: t.batteryC },
        ].map((x) => (
          <div key={x.k} className="rounded-lg bg-surface-2/70 px-2.5 py-1.5">
            <div className="text-[10.5px] uppercase tracking-wider text-ink-3">{x.k}</div>
            <MetricValue
              status={x.v == null && t.status === "ok" ? "unsupported" : t.status}
              value={x.v != null ? fmtTemp(x.v) : null}
              className="font-mono text-[16px] text-ink"
              title={x.k === "GPU" && x.v == null ? "No GPU-labelled sensor is exposed on this Mac" : undefined}
            />
          </div>
        ))}
      </div>
      {hottest.length > 0 && (
        <div className="mt-2">
          {hottest.map((h) => (
            <KV key={h.label} k={h.label} v={fmtTemp(h.celsius)} />
          ))}
          <div className="text-[10.5px] text-ink-3">{t.sensors.length} sensors via IOHID</div>
        </div>
      )}
      <div className="mt-auto flex items-start gap-2 border-t border-line pt-2 text-[11.5px]">
        <Fan size={14} className="mt-[1px] shrink-0 text-ink-3" />
        <div>
          <MetricValue status={f.status} value={f.rpm.length ? f.rpm.map((r) => `${r} RPM`).join(" · ") : null} className="text-ink" />
          {f.note && <div className="text-[11px] leading-snug text-ink-3">{f.note}</div>}
        </div>
      </div>
    </Card>
  );
}

function ProcessesCard({ s }: { s: SystemSnapshot }) {
  const procs = s.cpu.topProcesses;
  return (
    <Card title="Top processes (CPU)" icon={<ListOrdered size={15} />}>
      {procs.length === 0 ? (
        <Empty title="Sampling processes…" />
      ) : (
        <table className="w-full text-[12px]">
          <thead>
            <tr className="text-left text-[10.5px] uppercase tracking-wider text-ink-3">
              <th className="pb-1 font-medium">Process</th>
              <th className="pb-1 text-right font-medium">CPU</th>
              <th className="pb-1 text-right font-medium">Memory</th>
            </tr>
          </thead>
          <tbody>
            {procs.map((p) => (
              <tr key={p.pid} className="border-t border-line">
                <td className="max-w-[140px] truncate py-[5px] text-ink" title={`${p.name} (PID ${p.pid})`}>
                  {p.name}
                </td>
                <td className="py-[5px] text-right font-mono text-ink">{p.cpu.toFixed(1)}%</td>
                <td className="py-[5px] text-right font-mono text-ink-2">{fmtBytes(p.memory)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <div className="mt-auto pt-2 text-[10.5px] text-ink-3">% of one core, like Activity Monitor</div>
    </Card>
  );
}

export function Dashboard() {
  const t = useT();
  const snap = useSystem((s) => s.snapshot);
  const error = useSystem((s) => s.error);

  if (!snap) {
    return (
      <div className="flex h-full flex-col">
        {error ? <Notice tone="error">{error}</Notice> : <Empty title="Collecting first samples…">The sampler takes about two seconds to compute rates.</Empty>}
      </div>
    );
  }

  return (
    <div className="mx-auto flex max-w-[1500px] flex-col gap-4">
      <div className="flex items-baseline justify-between">
        <h1 className="text-[18px] font-semibold tracking-tight text-ink">{t("section.system")}</h1>
        {!snap.monitoring && <Badge tone="warn">monitoring paused · values frozen</Badge>}
      </div>
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 2xl:grid-cols-4">
        <CpuCard s={snap} />
        <MemoryCard s={snap} />
        <GpuCard s={snap} />
        <DiskCard s={snap} />
      </div>
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 2xl:grid-cols-4">
        <NetworkCard s={snap} />
        <BatteryCard s={snap} />
        <ThermalCard s={snap} />
        <ProcessesCard s={snap} />
      </div>
      <PerformanceChart />
    </div>
  );
}
