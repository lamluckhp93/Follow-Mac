export const NA = "N/A";

export function fmtPct(v: number | null | undefined, digits = 0): string {
  return v == null || !Number.isFinite(v) ? NA : `${v.toFixed(digits)}%`;
}

/** Binary units for memory/storage-style values (matches macOS Activity Monitor for RAM). */
export function fmtBytes(v: number | null | undefined, digits = 1): string {
  if (v == null || !Number.isFinite(v)) return NA;
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  let i = 0;
  let n = Math.abs(v);
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${v < 0 ? "-" : ""}${n.toFixed(i === 0 ? 0 : digits)} ${units[i]}`;
}

/** Decimal units for disk capacity (matches Finder / System Settings). */
export function fmtStorage(v: number | null | undefined, digits = 1): string {
  if (v == null || !Number.isFinite(v)) return NA;
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let n = v;
  while (n >= 1000 && i < units.length - 1) {
    n /= 1000;
    i++;
  }
  return `${n.toFixed(i === 0 ? 0 : digits)} ${units[i]}`;
}

export function fmtRate(bps: number | null | undefined): string {
  if (bps == null || !Number.isFinite(bps)) return NA;
  if (bps >= 1e9) return `${(bps / 1e9).toFixed(2)} GB/s`;
  if (bps >= 1e6) return `${(bps / 1e6).toFixed(1)} MB/s`;
  if (bps >= 1e3) return `${(bps / 1e3).toFixed(0)} KB/s`;
  return `${Math.round(bps)} B/s`;
}

export function fmtTokens(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return NA;
  const a = Math.abs(n);
  if (a >= 1e9) return `${(n / 1e9).toFixed(2)}B`;
  if (a >= 1e6) return `${(n / 1e6).toFixed(2)}M`;
  if (a >= 1e4) return `${(n / 1e3).toFixed(1)}K`;
  return n.toLocaleString("en-US");
}

export function fmtInt(n: number | null | undefined): string {
  return n == null || !Number.isFinite(n) ? NA : Math.round(n).toLocaleString("en-US");
}

export function fmtUsd(v: number | null | undefined): string {
  if (v == null || !Number.isFinite(v)) return NA;
  if (v === 0) return "$0.00";
  if (Math.abs(v) < 0.01) return `$${v.toFixed(4)}`;
  return `$${v.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
}

export function fmtTemp(c: number | null | undefined): string {
  return c == null || !Number.isFinite(c) ? NA : `${c.toFixed(0)}°C`;
}

export function fmtTime(ts: number | null | undefined, withSeconds = true): string {
  if (!ts) return NA;
  return new Date(ts).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: withSeconds ? "2-digit" : undefined,
  });
}

export function fmtDateTime(ts: number | null | undefined): string {
  if (!ts) return NA;
  return new Date(ts).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function fmtAgo(ts: number | null | undefined, now = Date.now()): string {
  if (!ts) return "never";
  const s = Math.max(0, Math.round((now - ts) / 1000));
  if (s < 5) return "just now";
  if (s < 60) return `${s}s ago`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.round(m / 60);
  if (h < 48) return `${h}h ago`;
  return `${Math.round(h / 24)}d ago`;
}

export function fmtDuration(min: number | null | undefined): string {
  if (min == null) return NA;
  const h = Math.floor(min / 60);
  const m = min % 60;
  return h > 0 ? `${h}h ${m}m` : `${m}m`;
}

export function fmtUptime(bootSec: number | null | undefined, now = Date.now()): string {
  if (!bootSec) return NA;
  const s = Math.floor(now / 1000 - bootSec);
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  return d > 0 ? `${d}d ${h}h` : `${h}h ${m}m`;
}
