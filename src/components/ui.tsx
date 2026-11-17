import { AlertTriangle, CheckCircle2, CircleDashed, CircleSlash, Info, Loader2, XCircle } from "lucide-react";
import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode } from "react";
import { STATUS } from "../lib/colors";
import { fmtTime } from "../lib/format";
import { useT } from "../lib/i18n";
import type { ConnectionStatus, MetricStatus } from "../lib/types";

export function cx(...c: (string | false | null | undefined)[]) {
  return c.filter(Boolean).join(" ");
}

export function Card({
  children,
  className,
  title,
  icon,
  right,
  footer,
}: {
  children: ReactNode;
  className?: string;
  title?: ReactNode;
  icon?: ReactNode;
  right?: ReactNode;
  footer?: ReactNode;
}) {
  return (
    <section className={cx("glass card-pad animate-in flex min-w-0 flex-col rounded-2xl p-4", className)}>
      {(title || right) && (
        <header className="mb-3 flex items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-2 text-[13px] font-medium tracking-wide text-ink-2">
            {icon && <span className="accent-text flex shrink-0">{icon}</span>}
            <span className="truncate">{title}</span>
          </div>
          {right && <div className="flex shrink-0 items-center gap-2">{right}</div>}
        </header>
      )}
      <div className="flex min-h-0 flex-1 flex-col">{children}</div>
      {footer && <footer className="mt-3 border-t border-line pt-2 text-[11px] text-ink-3">{footer}</footer>}
    </section>
  );
}

export function Updated({ ts }: { ts: number | null | undefined }) {
  const t = useT();
  return (
    <span className="font-mono text-[11px] text-ink-3" title={ts ? new Date(ts).toISOString() : undefined}>
      {t("updated")} {ts ? fmtTime(ts) : "—"}
    </span>
  );
}

const STATUS_STYLE: Record<ConnectionStatus, { color: string; icon: ReactNode }> = {
  connected: { color: STATUS.good, icon: <CheckCircle2 size={12} /> },
  syncing: { color: "var(--accent)", icon: <Loader2 size={12} className="animate-spin" /> },
  error: { color: STATUS.critical, icon: <XCircle size={12} /> },
  unsupported: { color: STATUS.warning, icon: <CircleSlash size={12} /> },
  not_configured: { color: "#6b7591", icon: <CircleDashed size={12} /> },
  disabled: { color: "#6b7591", icon: <CircleSlash size={12} /> },
};

export function StatusPill({ status }: { status: ConnectionStatus }) {
  const t = useT();
  const s = STATUS_STYLE[status];
  return (
    <span
      className="inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[11px] font-medium text-ink"
      style={{ borderColor: `color-mix(in srgb, ${s.color} 45%, transparent)`, background: `color-mix(in srgb, ${s.color} 12%, transparent)` }}
    >
      <span style={{ color: s.color }} className="flex">
        {s.icon}
      </span>
      {t(`status.${status}`)}
    </span>
  );
}

export function Badge({ children, tone = "neutral", title }: { children: ReactNode; tone?: "neutral" | "accent" | "warn" | "good" | "bad"; title?: string }) {
  const color =
    tone === "accent" ? "var(--accent)" : tone === "warn" ? STATUS.warning : tone === "good" ? STATUS.good : tone === "bad" ? STATUS.critical : "#6b7591";
  return (
    <span
      title={title}
      className="inline-flex items-center gap-1 whitespace-nowrap rounded-md border px-1.5 py-0.5 text-[10.5px] font-medium uppercase tracking-wider text-ink-2"
      style={{ borderColor: `color-mix(in srgb, ${color} 40%, transparent)`, background: `color-mix(in srgb, ${color} 10%, transparent)` }}
    >
      {children}
    </span>
  );
}

const SOURCE_TONE: Record<string, "good" | "accent" | "neutral" | "warn"> = {
  provider_reported: "good",
  provider_reported_cost: "good",
  telemetry: "accent",
  imported: "neutral",
  imported_cost: "neutral",
  estimated: "warn",
};

export function SourceBadge({ source }: { source: string }) {
  const t = useT();
  const base = source.replace(/_cost$/, "");
  const label =
    base === "provider_reported" || base === "telemetry" || base === "imported" || base === "estimated" ? t(`source.${base}`) : base;
  return (
    <Badge tone={SOURCE_TONE[source] ?? "neutral"} title={source.endsWith("_cost") ? "Cost data source" : "Token data source"}>
      {source.endsWith("_cost") ? "$ " : ""}
      {label}
    </Badge>
  );
}

/** Renders a metric value, or an explicit status label when the value is missing. */
export function MetricValue({
  status,
  value,
  className,
  title,
}: {
  status: MetricStatus;
  value: ReactNode | null;
  className?: string;
  title?: string;
}) {
  const t = useT();
  if (value != null && status !== "unsupported" && status !== "not_present") {
    return <span className={className}>{value}</span>;
  }
  const label =
    status === "unsupported" ? t("metric.unsupported") : status === "not_present" ? t("metric.not_present") : status === "pending" ? t("metric.pending") : t("metric.unavailable");
  return (
    <span className={cx(className, "!text-ink-3")} title={title}>
      {label}
    </span>
  );
}

export function Segmented<T extends string | number>({
  value,
  options,
  onChange,
  size = "sm",
}: {
  value: T;
  options: readonly { key: T; label: string }[];
  onChange: (v: T) => void;
  size?: "sm" | "md";
}) {
  return (
    <div className="inline-flex rounded-lg border border-line bg-bg-2/70 p-0.5" role="tablist">
      {options.map((o) => (
        <button
          key={String(o.key)}
          role="tab"
          aria-selected={o.key === value}
          onClick={() => onChange(o.key)}
          className={cx(
            "accent-ring rounded-md font-medium transition-colors",
            size === "sm" ? "px-2 py-0.5 text-[11px]" : "px-3 py-1 text-[12px]",
            o.key === value ? "bg-surface-3 text-ink shadow-sm" : "text-ink-3 hover:text-ink-2",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Toggle({ checked, onChange, disabled, label }: { checked: boolean; onChange: (v: boolean) => void; disabled?: boolean; label?: string }) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cx(
        "accent-ring relative inline-flex h-5 w-9 shrink-0 items-center rounded-full border transition-colors disabled:opacity-40",
        checked ? "border-transparent accent-bg" : "border-line-strong bg-surface-3",
      )}
    >
      <span className={cx("inline-block h-3.5 w-3.5 rounded-full bg-white shadow transition-transform", checked ? "translate-x-[18px]" : "translate-x-[3px]")} />
    </button>
  );
}

export function Button({
  variant = "secondary",
  loading,
  children,
  className,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "primary" | "secondary" | "ghost" | "danger"; loading?: boolean }) {
  return (
    <button
      {...rest}
      disabled={rest.disabled || loading}
      className={cx(
        "accent-ring inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-lg px-3 py-1.5 text-[12px] font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-45",
        variant === "primary" && "accent-bg text-[#06111a] hover:brightness-110",
        variant === "secondary" && "border border-line-strong bg-surface-2 text-ink hover:bg-surface-3",
        variant === "ghost" && "text-ink-2 hover:bg-surface-2 hover:text-ink",
        variant === "danger" && "border border-[#d03b3b]/50 bg-[#d03b3b]/10 text-[#f19c9c] hover:bg-[#d03b3b]/20",
        className,
      )}
    >
      {loading && <Loader2 size={13} className="animate-spin" />}
      {children}
    </button>
  );
}

export function TextInput(props: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      {...props}
      className={cx(
        "accent-ring rounded-lg border border-line-strong bg-bg-2 px-2.5 py-1.5 text-[12.5px] text-ink placeholder:text-ink-3 focus:border-[var(--accent)] focus:outline-none",
        // Full width unless the caller sets an explicit width.
        /(^|\s)w-/.test(props.className ?? "") ? "shrink-0" : "w-full",
        props.className,
      )}
    />
  );
}

export function NumberField({
  value,
  onChange,
  min,
  max,
  step = 1,
  suffix,
  width = "w-24",
}: {
  value: number;
  onChange: (v: number) => void;
  min?: number;
  max?: number;
  step?: number;
  suffix?: string;
  width?: string;
}) {
  return (
    <div className="inline-flex items-center gap-1.5">
      <TextInput
        type="number"
        className={width}
        value={Number.isFinite(value) ? value : ""}
        min={min}
        max={max}
        step={step}
        onChange={(e) => {
          const v = Number(e.target.value);
          if (Number.isFinite(v)) onChange(v);
        }}
      />
      {suffix && <span className="text-[11px] text-ink-3">{suffix}</span>}
    </div>
  );
}

export function Row({ label, hint, children }: { label: ReactNode; hint?: ReactNode; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-6 border-b border-line py-3 last:border-0">
      <div className="min-w-0">
        <div className="text-[13px] text-ink">{label}</div>
        {hint && <div className="mt-0.5 text-[11.5px] leading-snug text-ink-3">{hint}</div>}
      </div>
      <div className="flex shrink-0 items-center gap-2">{children}</div>
    </div>
  );
}

export function KV({ k, v, title }: { k: ReactNode; v: ReactNode; title?: string }) {
  return (
    <div className="flex items-baseline justify-between gap-3 py-[3px] text-[12px]" title={title}>
      <span className="truncate text-ink-3">{k}</span>
      <span className="shrink-0 font-mono text-ink">{v}</span>
    </div>
  );
}

export function Notice({ tone = "info", children }: { tone?: "info" | "warn" | "error" | "good"; children: ReactNode }) {
  const color = tone === "warn" ? STATUS.warning : tone === "error" ? STATUS.critical : tone === "good" ? STATUS.good : "var(--accent)";
  const Icon = tone === "warn" ? AlertTriangle : tone === "error" ? XCircle : tone === "good" ? CheckCircle2 : Info;
  return (
    <div
      className="selectable flex items-start gap-2 rounded-lg border px-3 py-2 text-[12px] leading-snug text-ink-2"
      style={{ borderColor: `color-mix(in srgb, ${color} 35%, transparent)`, background: `color-mix(in srgb, ${color} 8%, transparent)` }}
    >
      <Icon size={14} className="mt-[1px] shrink-0" style={{ color }} />
      <div className="min-w-0 break-words">{children}</div>
    </div>
  );
}

export function Empty({ icon, title, children }: { icon?: ReactNode; title: string; children?: ReactNode }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 px-6 py-8 text-center">
      {icon && <div className="text-ink-3">{icon}</div>}
      <div className="text-[13px] font-medium text-ink-2">{title}</div>
      {children && <div className="max-w-md text-[12px] leading-relaxed text-ink-3">{children}</div>}
    </div>
  );
}

export function Bar({ value, color = "var(--accent)", height = 6 }: { value: number | null; color?: string; height?: number }) {
  const v = value == null ? 0 : Math.max(0, Math.min(100, value));
  return (
    <div className="w-full overflow-hidden rounded-full bg-surface-3" style={{ height }}>
      <div className="h-full rounded-full transition-[width] duration-500" style={{ width: `${v}%`, background: color }} />
    </div>
  );
}
