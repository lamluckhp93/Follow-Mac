import { ask } from "@tauri-apps/plugin-dialog";
import { CheckCircle2, KeyRound, Plug, Send, Trash2, XCircle } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Badge, Button, Card, KV, Notice, NumberField, Row, Segmented, StatusPill, TextInput, Toggle, cx } from "../components/ui";
import { api, errorMessage } from "../lib/api";
import { PROVIDER_COLORS, PROVIDER_LABEL } from "../lib/colors";
import { fmtInt, fmtUsd } from "../lib/format";
import type { ConnectionTest, ModelPrice, ProviderId, ProviderView, ProxyStatus, TestRequestResult } from "../lib/types";
import { useAi } from "../store/ai";
import { useSettings } from "../store/settings";

const INTERVALS = [15, 30, 60, 180, 360, 1440];

function KeyField({ provider, kind, label, has }: { provider: ProviderId; kind: "api" | "admin"; label: string; has: boolean }) {
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const save = async () => {
    setBusy(true);
    setErr(null);
    try {
      await api.setProviderKey(provider, kind, value);
      setValue("");
    } catch (e) {
      setErr(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const remove = async () => {
    if (!(await ask(`Remove the ${label.toLowerCase()} for ${PROVIDER_LABEL[provider]} from the Keychain?`, { title: "Remove key", kind: "warning" }))) return;
    try {
      await api.removeProviderKey(provider, kind);
    } catch (e) {
      setErr(errorMessage(e));
    }
  };
  return (
    <div className="py-2">
      <div className="mb-1 flex items-center justify-between">
        <span className="flex items-center gap-1.5 text-[12.5px] text-ink">
          <KeyRound size={13} className="text-ink-3" /> {label}
        </span>
        {has ? <Badge tone="good">stored in Keychain</Badge> : <Badge>not set</Badge>}
      </div>
      <div className="flex gap-2">
        <TextInput
          type="password"
          autoComplete="off"
          spellCheck={false}
          placeholder={has ? "•••••••• (enter a new key to replace)" : "Paste key"}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && value && save()}
        />
        <Button variant="primary" disabled={!value} loading={busy} onClick={save}>
          Save
        </Button>
        {has && (
          <Button variant="ghost" onClick={remove} title="Remove from Keychain">
            <Trash2 size={13} />
          </Button>
        )}
      </div>
      {err && <div className="mt-1 text-[11.5px] text-[#f19c9c]">{err}</div>}
    </div>
  );
}

function ProviderPanel({ p, proxy }: { p: ProviderView; proxy: ProxyStatus | null }) {
  const [cfg, setCfg] = useState({ enabled: p.enabled, interval: p.syncIntervalMinutes, baseUrl: p.baseUrl ?? "", models: p.selectedModels });
  const [saving, setSaving] = useState(false);
  const [saveMsg, setSaveMsg] = useState<{ tone: "good" | "error"; text: string } | null>(null);
  const [test, setTest] = useState<ConnectionTest | null>(null);
  const [testing, setTesting] = useState(false);
  const [testErr, setTestErr] = useState<string | null>(null);
  const [model, setModel] = useState(p.selectedModels[0] ?? "");
  const [req, setReq] = useState<TestRequestResult | null>(null);
  const [reqBusy, setReqBusy] = useState(false);
  const [reqErr, setReqErr] = useState<string | null>(null);
  const [modelFilter, setModelFilter] = useState("");

  useEffect(() => {
    setCfg({ enabled: p.enabled, interval: p.syncIntervalMinutes, baseUrl: p.baseUrl ?? "", models: p.selectedModels });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [p.id, p.enabled, p.syncIntervalMinutes, p.baseUrl, p.selectedModels.join("|")]);

  const save = async (next = cfg) => {
    setSaving(true);
    setSaveMsg(null);
    try {
      await api.saveProviderConfig({ id: p.id, enabled: next.enabled, syncIntervalMinutes: next.interval, baseUrl: next.baseUrl || null, selectedModels: next.models });
      setSaveMsg({ tone: "good", text: "Saved." });
    } catch (e) {
      setSaveMsg({ tone: "error", text: errorMessage(e) });
    } finally {
      setSaving(false);
    }
  };

  const runTest = async () => {
    setTesting(true);
    setTest(null);
    setTestErr(null);
    try {
      setTest(await api.testProvider(p.id));
    } catch (e) {
      setTestErr(errorMessage(e));
    } finally {
      setTesting(false);
    }
  };

  const sendReq = async () => {
    const ok = await ask(
      `Send one tiny request (~20 tokens) to ${model} on ${PROVIDER_LABEL[p.id]}? It is billed to your account at normal rates and its usage is recorded as telemetry.`,
      { title: "Send test request", kind: "info" },
    );
    if (!ok) return;
    setReqBusy(true);
    setReq(null);
    setReqErr(null);
    try {
      setReq(await api.sendTestRequest(p.id, model));
    } catch (e) {
      setReqErr(errorMessage(e));
    } finally {
      setReqBusy(false);
    }
  };

  const removeAll = async () => {
    if (!(await ask(`Remove all ${PROVIDER_LABEL[p.id]} credentials from the Keychain and disable the provider? Usage history is kept.`, { title: "Remove credentials", kind: "warning" })))
      return;
    try {
      await api.removeProviderCredentials(p.id);
      setTest(null);
    } catch (e) {
      setSaveMsg({ tone: "error", text: errorMessage(e) });
    }
  };

  const toggleModel = (m: string) => {
    const models = cfg.models.includes(m) ? cfg.models.filter((x) => x !== m) : [...cfg.models, m];
    setCfg({ ...cfg, models });
  };
  const available = useMemo(() => {
    const all = Array.from(new Set([...(test?.models ?? []), ...cfg.models]));
    const f = modelFilter.toLowerCase();
    return all.filter((m) => !f || m.toLowerCase().includes(f)).slice(0, 200);
  }, [test, cfg.models, modelFilter]);
  const dirty = cfg.enabled !== p.enabled || cfg.interval !== p.syncIntervalMinutes || cfg.baseUrl !== (p.baseUrl ?? "") || cfg.models.join("|") !== p.selectedModels.join("|");
  const caps = p.capabilities;

  return (
    <div className="flex flex-col gap-4">
      <Card
        title={
          <span className="flex items-center gap-2 text-ink">
            <span className="h-2.5 w-2.5 rounded-sm" style={{ background: PROVIDER_COLORS[p.id] }} /> {p.displayName}
          </span>
        }
        right={<StatusPill status={p.status} />}
      >
        <Notice>
          {caps.notes} <span className="selectable font-mono text-ink-3">{caps.docsUrl}</span>
        </Notice>
        <div className="mt-2 flex flex-wrap gap-1.5">
          <Badge tone={caps.usageApi ? "good" : "neutral"}>Usage API {caps.usageApi ? "✓" : "✗"}</Badge>
          <Badge tone={caps.costApi ? "good" : "neutral"}>Cost data {caps.costApi ? "✓" : "✗"}</Badge>
          <Badge tone={caps.accountInfo ? "good" : "neutral"}>Account/balance {caps.accountInfo ? "✓" : "✗"}</Badge>
          <Badge tone="accent">Telemetry ✓</Badge>
        </div>
        <div className="mt-2">
          <Row label="Enabled" hint="Enabled providers are synced on schedule and counted in the menu bar.">
            <Toggle checked={cfg.enabled} onChange={(v) => setCfg({ ...cfg, enabled: v })} />
          </Row>
          {(caps.usageApi || caps.costApi || caps.accountInfo) && (
            <Row label="Sync interval" hint="How often official usage/credit data is fetched.">
              <Segmented
                value={cfg.interval}
                onChange={(v) => setCfg({ ...cfg, interval: v })}
                options={INTERVALS.map((m) => ({ key: m, label: m < 60 ? `${m}m` : `${m / 60}h` }))}
              />
            </Row>
          )}
          {caps.needsBaseUrl && (
            <Row label="Base URL" hint="OpenAI-compatible endpoint including /v1, e.g. http://localhost:11434/v1 (http only for localhost).">
              <TextInput className="w-[300px]" value={cfg.baseUrl} placeholder="https://…/v1" onChange={(e) => setCfg({ ...cfg, baseUrl: e.target.value })} />
            </Row>
          )}
        </div>
        <div className="mt-2 flex items-center gap-2">
          <Button variant="primary" disabled={!dirty} loading={saving} onClick={() => save()}>
            Save configuration
          </Button>
          {saveMsg && <span className={cx("text-[12px]", saveMsg.tone === "good" ? "text-[#7fd67f]" : "text-[#f19c9c]")}>{saveMsg.text}</span>}
        </div>
      </Card>

      <Card title="Credentials (macOS Keychain)" icon={<KeyRound size={15} />} footer="Keys are written directly to your login Keychain by the Rust backend. They are never stored in the database, logs or the web view.">
        <KeyField provider={p.id} kind="api" label={p.id === "custom" ? "API key (optional)" : "API key"} has={p.hasApiKey} />
        {caps.adminKeyLabel && <KeyField provider={p.id} kind="admin" label={caps.adminKeyLabel} has={p.hasAdminKey} />}
        <div className="mt-2 flex flex-wrap gap-2">
          <Button onClick={runTest} loading={testing}>
            <Plug size={13} /> Test connection
          </Button>
          {(p.hasApiKey || p.hasAdminKey) && (
            <Button variant="danger" onClick={removeAll}>
              <Trash2 size={13} /> Remove credentials
            </Button>
          )}
        </div>
        {testErr && (
          <div className="mt-2">
            <Notice tone="error">{testErr}</Notice>
          </div>
        )}
        {test && (
          <div className="mt-2 flex flex-col gap-1.5">
            <Notice tone={test.ok ? "good" : "error"}>
              {test.message} · {test.latencyMs} ms
            </Notice>
            {test.usageApi && <Notice tone={test.usageApi.ok ? "good" : "warn"}>Usage API: {test.usageApi.message}</Notice>}
          </div>
        )}
      </Card>

      <Card title="Models" footer="Selected models appear first for test requests. Run “Test connection” to load the provider's model list.">
        <TextInput placeholder="Filter or type a model id and press Enter" value={modelFilter} onChange={(e) => setModelFilter(e.target.value)} onKeyDown={(e) => {
          if (e.key === "Enter" && modelFilter.trim() && !cfg.models.includes(modelFilter.trim())) {
            setCfg({ ...cfg, models: [...cfg.models, modelFilter.trim()] });
            setModelFilter("");
          }
        }} />
        <div className="mt-2 flex max-h-[180px] flex-wrap gap-1.5 overflow-y-auto">
          {available.length === 0 && <span className="text-[12px] text-ink-3">No models loaded.</span>}
          {available.map((m) => (
            <button
              key={m}
              onClick={() => toggleModel(m)}
              className={cx(
                "accent-ring rounded-md border px-2 py-0.5 font-mono text-[11px]",
                cfg.models.includes(m) ? "border-[var(--accent)] bg-[var(--accent-soft)] text-ink" : "border-line-strong text-ink-2 hover:text-ink",
              )}
            >
              {m}
            </button>
          ))}
        </div>
        {dirty && (
          <div className="mt-2">
            <Button variant="primary" loading={saving} onClick={() => save()}>
              Save model selection
            </Button>
          </div>
        )}
      </Card>

      <Card title="Request telemetry" icon={<Send size={15} />}>
        <div className="text-[12px] leading-relaxed text-ink-2">
          Send a minimal request to verify the integration end-to-end. Token counts are read from the response's usage metadata and stored as
          <Badge tone="accent">API telemetry</Badge>. Prompt and response text are never stored.
        </div>
        <div className="mt-2 flex gap-2">
          <TextInput list={`models-${p.id}`} placeholder="model id" value={model} onChange={(e) => setModel(e.target.value)} className="font-mono" />
          <datalist id={`models-${p.id}`}>
            {[...cfg.models, ...(test?.models ?? [])].slice(0, 200).map((m) => (
              <option key={m} value={m} />
            ))}
          </datalist>
          <Button onClick={sendReq} loading={reqBusy} disabled={!model.trim() || p.status === "not_configured"}>
            <Send size={13} /> Send test request
          </Button>
        </div>
        {reqErr && (
          <div className="mt-2">
            <Notice tone="error">{reqErr}</Notice>
          </div>
        )}
        {req && (
          <div className="mt-2">
            <Notice tone={req.ok ? "good" : "error"}>
              <div className="flex items-center gap-1.5">
                {req.ok ? <CheckCircle2 size={13} /> : <XCircle size={13} />} HTTP {req.status} · {req.model} · {req.latencyMs} ms — {req.message}
              </div>
              {req.usage && (
                <div className="mt-1 font-mono">
                  input {fmtInt(req.usage.input)} · cached {fmtInt(req.usage.cached)} · cache write {fmtInt(req.usage.cacheWrite)} · output {fmtInt(req.usage.output)}
                  {req.usage.costUsd != null && ` · ${fmtUsd(req.usage.costUsd)}`}
                </div>
              )}
            </Notice>
          </div>
        )}
        {proxy?.running && p.proxyBaseUrl && (
          <div className="mt-3 border-t border-line pt-2">
            <KV k="Proxy base URL for your SDK" v={<span className="selectable">{p.proxyBaseUrl}</span>} />
          </div>
        )}
      </Card>
    </div>
  );
}

function PricingEditor() {
  const [prices, setPrices] = useState<ModelPrice[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [draft, setDraft] = useState<ModelPrice>({
    provider: "openai",
    model: "",
    inputPrice: null,
    outputPrice: null,
    cachedInputPrice: null,
    cacheWritePrice: null,
    priceSource: "user",
    updatedAt: 0,
  });
  const load = () => api.prices().then(setPrices).catch((e) => setErr(errorMessage(e)));
  useEffect(() => {
    void load();
  }, []);
  const save = async (p: ModelPrice) => {
    setErr(null);
    try {
      await api.upsertPrice(p);
      await load();
      return true;
    } catch (e) {
      setErr(errorMessage(e));
      return false;
    }
  };
  const del = async (p: ModelPrice) => {
    if (!(await ask(`Delete the price for ${p.model}? Its usage will show as unpriced.`, { title: "Delete price", kind: "warning" }))) return;
    try {
      await api.deletePrice(p.provider, p.model);
      await load();
    } catch (e) {
      setErr(errorMessage(e));
    }
  };
  const num = (v: string) => (v.trim() === "" ? null : Number(v));
  const cell = (p: ModelPrice, k: keyof Pick<ModelPrice, "inputPrice" | "outputPrice" | "cachedInputPrice" | "cacheWritePrice">) => (
    <input
      type="number"
      min={0}
      step="0.01"
      defaultValue={p[k] ?? ""}
      onBlur={(e) => {
        const v = num(e.target.value);
        if (v !== p[k]) void save({ ...p, [k]: v });
      }}
      className="accent-ring w-20 rounded-md border border-line bg-bg-2 px-1.5 py-0.5 text-right font-mono text-[11.5px] text-ink focus:border-[var(--accent)] focus:outline-none"
    />
  );
  return (
    <Card title="Pricing table (estimated cost)" footer="USD per 1M tokens. Used only for “Estimated” cost when the provider does not report actual cost. Anthropic list prices are pre-filled (cached 2026-11-03); other providers start empty — enter current prices from their pricing pages.">
      {err && <Notice tone="error">{err}</Notice>}
      <div className="max-h-[360px] overflow-y-auto">
        <table className="w-full text-[12px]">
          <thead className="sticky top-0 bg-surface">
            <tr className="text-left text-[10.5px] uppercase tracking-wider text-ink-3">
              <th className="py-1 font-medium">Provider</th>
              <th className="py-1 font-medium">Model (prefix match)</th>
              <th className="py-1 text-right font-medium">Input</th>
              <th className="py-1 text-right font-medium">Output</th>
              <th className="py-1 text-right font-medium">Cached in</th>
              <th className="py-1 text-right font-medium">Cache write</th>
              <th className="py-1 font-medium">Source</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {prices?.map((p) => (
              <tr key={`${p.provider}/${p.model}/${p.updatedAt}`} className="border-t border-line">
                <td className="py-1 text-ink-2">{PROVIDER_LABEL[p.provider] ?? p.provider}</td>
                <td className="py-1 font-mono text-ink">{p.model}</td>
                <td className="py-1 text-right">{cell(p, "inputPrice")}</td>
                <td className="py-1 text-right">{cell(p, "outputPrice")}</td>
                <td className="py-1 text-right">{cell(p, "cachedInputPrice")}</td>
                <td className="py-1 text-right">{cell(p, "cacheWritePrice")}</td>
                <td className="py-1 text-[11px] text-ink-3">{p.priceSource}</td>
                <td className="py-1 text-right">
                  <button className="text-ink-3 hover:text-[#f19c9c]" onClick={() => del(p)} aria-label="Delete price">
                    <Trash2 size={13} />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-2 border-t border-line pt-3">
        <select
          value={draft.provider}
          onChange={(e) => setDraft({ ...draft, provider: e.target.value as ProviderId })}
          className="accent-ring rounded-lg border border-line-strong bg-bg-2 px-2 py-1.5 text-[12px] text-ink"
        >
          {(Object.keys(PROVIDER_LABEL) as ProviderId[]).map((p) => (
            <option key={p} value={p}>
              {PROVIDER_LABEL[p]}
            </option>
          ))}
        </select>
        <TextInput className="w-[180px] font-mono" placeholder="model id" value={draft.model} onChange={(e) => setDraft({ ...draft, model: e.target.value })} />
        {(["inputPrice", "outputPrice", "cachedInputPrice", "cacheWritePrice"] as const).map((k) => (
          <TextInput
            key={k}
            type="number"
            min={0}
            step="0.01"
            className="w-[92px]"
            placeholder={{ inputPrice: "input", outputPrice: "output", cachedInputPrice: "cached", cacheWritePrice: "cache write" }[k]}
            value={draft[k] ?? ""}
            onChange={(e) => setDraft({ ...draft, [k]: num(e.target.value) })}
          />
        ))}
        <Button
          variant="primary"
          disabled={!draft.model.trim() || draft.inputPrice == null || draft.outputPrice == null}
          onClick={async () => {
            if (await save(draft)) setDraft({ ...draft, model: "", inputPrice: null, outputPrice: null, cachedInputPrice: null, cacheWritePrice: null });
          }}
        >
          Add price
        </Button>
      </div>
    </Card>
  );
}

function ProxyCard({ proxy, reload }: { proxy: ProxyStatus | null; reload: () => void }) {
  const settings = useSettings((s) => s.settings);
  const update = useSettings((s) => s.update);
  const [port, setPort] = useState(settings?.ai.proxyPort ?? 47821);
  const [err, setErr] = useState<string | null>(null);
  if (!settings) return null;
  const set = async (fn: Parameters<typeof update>[0]) => {
    setErr(await update(fn));
    setTimeout(reload, 400);
  };
  return (
    <Card title="Local telemetry proxy" icon={<Plug size={15} />}>
      <div className="text-[12px] leading-relaxed text-ink-2">
        Point your SDK or tool at <span className="selectable font-mono text-ink">http://127.0.0.1:{settings.ai.proxyPort}/&lt;provider&gt;/…</span> and requests are forwarded unchanged
        to the provider while token usage is read from each response (streaming supported). Your client keeps sending its own API key; the proxy never
        injects keys and never stores content. Binds to 127.0.0.1 only.
      </div>
      <Row label="Enable proxy" hint={proxy?.running ? `Listening on 127.0.0.1:${proxy.port}` : proxy?.error ?? "Stopped"}>
        <Toggle checked={settings.ai.proxyEnabled} onChange={(v) => set((s) => void (s.ai.proxyEnabled = v))} />
      </Row>
      <Row label="Port">
        <NumberField value={port} min={1024} max={65535} onChange={setPort} />
        <Button disabled={port === settings.ai.proxyPort} onClick={() => set((s) => void (s.ai.proxyPort = Math.round(port)))}>
          Apply
        </Button>
      </Row>
      {err && <Notice tone="error">{err}</Notice>}
      {proxy?.error && <Notice tone="error">{proxy.error}</Notice>}
      <div className="mt-1 text-[11.5px] text-ink-3">
        Examples: OpenAI SDK <span className="selectable font-mono">base_url=http://127.0.0.1:{settings.ai.proxyPort}/openai/v1</span> · Anthropic SDK{" "}
        <span className="selectable font-mono">base_url=http://127.0.0.1:{settings.ai.proxyPort}/anthropic</span>. For OpenAI-style streams, pass{" "}
        <span className="font-mono">stream_options: {"{include_usage: true}"}</span> so the final chunk carries usage.
      </div>
    </Card>
  );
}

function UsageSourceCard() {
  const settings = useSettings((s) => s.settings);
  const update = useSettings((s) => s.update);
  if (!settings) return null;
  return (
    <Card title="Usage source">
      <Row
        label="Count tokens from"
        hint="Auto picks the best source per provider and UTC day (provider reported > imported > telemetry) so nothing is double counted."
      >
        <Segmented
          value={settings.ai.usageSource}
          onChange={(v) => update((s) => void (s.ai.usageSource = v))}
          options={[
            { key: "auto", label: "Auto" },
            { key: "provider_reported", label: "Provider" },
            { key: "telemetry", label: "Telemetry" },
            { key: "imported", label: "Imported" },
          ]}
        />
      </Row>
    </Card>
  );
}

export function ProviderSettings({ initialProvider }: { initialProvider?: string }) {
  const providers = useAi((s) => s.providers);
  const [sel, setSel] = useState<ProviderId>((initialProvider as ProviderId) ?? "openai");
  const [proxy, setProxy] = useState<ProxyStatus | null>(null);
  const loadProxy = () => api.proxyStatus().then(setProxy).catch(() => setProxy(null));
  useEffect(() => {
    void loadProxy();
  }, []);
  const p = providers.find((x) => x.id === sel);
  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-[220px_1fr] gap-4">
        <div className="flex flex-col gap-1">
          {providers.map((x) => (
            <button
              key={x.id}
              onClick={() => setSel(x.id)}
              className={cx(
                "accent-ring flex items-center justify-between gap-2 rounded-xl border px-3 py-2 text-left text-[12.5px]",
                sel === x.id ? "border-line-strong bg-surface-2 text-ink" : "border-transparent text-ink-2 hover:bg-surface-2/60",
              )}
            >
              <span className="flex items-center gap-2">
                <span className="h-2.5 w-2.5 rounded-sm" style={{ background: PROVIDER_COLORS[x.id] }} />
                {PROVIDER_LABEL[x.id]}
              </span>
              <span className="text-[10px] text-ink-3">{x.status === "not_configured" ? "—" : x.enabled ? "on" : "off"}</span>
            </button>
          ))}
        </div>
        <div className="min-w-0">{p && <ProviderPanel key={p.id} p={p} proxy={proxy} />}</div>
      </div>
      <ProxyCard proxy={proxy} reload={loadProxy} />
      <UsageSourceCard />
      <PricingEditor />
    </div>
  );
}
