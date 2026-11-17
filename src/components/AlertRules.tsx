import { useEffect, useState } from "react";
import { api, errorMessage } from "../lib/api";
import type { AlertRule } from "../lib/types";
import { Badge, Button, Notice, NumberField, Toggle } from "./ui";

const META: Record<string, { label: string; unit: string; hint: string; step?: number }> = {
  cpu_high: { label: "CPU above", unit: "%", hint: "Total CPU usage" },
  memory_high: { label: "Memory above", unit: "%", hint: "App + wired + compressed ÷ total" },
  memory_pressure: { label: "Memory pressure at least", unit: "level", hint: "macOS pressure level: 2 = warning, 4 = critical" },
  battery_low: { label: "Battery below", unit: "%", hint: "Only when not charging / not on power" },
  disk_full: { label: "Startup disk above", unit: "%", hint: "Used capacity of the data volume" },
  tokens_daily: { label: "Tokens today above", unit: "tokens", hint: "All providers, local day", step: 10000 },
  cost_daily: { label: "Daily cost above", unit: "USD", hint: "Actual + estimated, local day", step: 1 },
  cost_monthly: { label: "Monthly cost above", unit: "USD", hint: "Actual + estimated, calendar month", step: 5 },
  provider_error: { label: "Provider sync fails", unit: "", hint: "Network / server errors during usage sync" },
  auth_error: { label: "API key rejected", unit: "", hint: "HTTP 401/403 from a provider (expired or revoked key)" },
};

function RuleRow({ rule, onSaved }: { rule: AlertRule; onSaved: () => void }) {
  const [r, setR] = useState(rule);
  const [saving, setSaving] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const meta = META[rule.id] ?? { label: rule.id, unit: "", hint: "" };
  const dirty = JSON.stringify(r) !== JSON.stringify(rule);
  const hasThreshold = !["provider_error", "auth_error"].includes(rule.id);

  const save = async (next: AlertRule) => {
    setSaving(true);
    setErr(null);
    try {
      await api.updateAlertRule(next);
      onSaved();
    } catch (e) {
      setErr(errorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="border-b border-line py-2.5 last:border-0">
      <div className="flex flex-wrap items-center gap-3">
        <Toggle
          checked={r.enabled}
          label={meta.label}
          onChange={(v) => {
            const next = { ...r, enabled: v };
            setR(next);
            void save(next);
          }}
        />
        <div className="min-w-[180px] flex-1">
          <div className="text-[13px] text-ink">{meta.label}</div>
          <div className="text-[11px] text-ink-3">{meta.hint}</div>
        </div>
        {hasThreshold && <NumberField value={r.threshold} min={0} step={meta.step ?? 1} suffix={meta.unit} onChange={(v) => setR({ ...r, threshold: v })} />}
        {r.category === "system" && (
          <NumberField value={r.durationS} min={0} max={86400} suffix="s sustained" width="w-20" onChange={(v) => setR({ ...r, durationS: Math.round(v) })} />
        )}
        <NumberField value={Math.round(r.cooldownS / 60)} min={0} max={10080} suffix="min cooldown" width="w-20" onChange={(v) => setR({ ...r, cooldownS: Math.round(v * 60) })} />
        <Button variant={dirty ? "primary" : "secondary"} disabled={!dirty} loading={saving} onClick={() => save(r)}>
          Save
        </Button>
      </div>
      {err && (
        <div className="mt-2">
          <Notice tone="error">{err}</Notice>
        </div>
      )}
    </div>
  );
}

export function AlertRulesEditor({ category }: { category?: "system" | "ai" }) {
  const [rules, setRules] = useState<AlertRule[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = () =>
    api
      .alertRules()
      .then(setRules)
      .catch((e) => setError(errorMessage(e)));
  useEffect(() => {
    void load();
  }, []);
  if (error) return <Notice tone="error">{error}</Notice>;
  if (!rules) return null;
  const groups = (["system", "ai"] as const).filter((g) => !category || g === category);
  return (
    <div className="flex flex-col gap-4">
      {groups.map((g) => (
        <div key={g}>
          <div className="mb-1 flex items-center gap-2">
            <Badge tone="accent">{g === "system" ? "System" : "AI usage"}</Badge>
          </div>
          {rules
            .filter((r) => r.category === g)
            .map((r) => (
              <RuleRow key={`${r.id}-${r.threshold}-${r.enabled}-${r.cooldownS}-${r.durationS}`} rule={r} onSaved={load} />
            ))}
        </div>
      ))}
    </div>
  );
}
