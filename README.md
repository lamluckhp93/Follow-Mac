# Mac Observer

**Mac System & AI Usage Dashboard** — a native macOS app (Tauri 2 + Rust + React) that monitors your Mac in real time and tracks token usage and cost across AI providers, in one dark, futuristic dashboard, a menu bar item and a floating widget.

Everything shown is real: system values come from macOS kernel/IOKit interfaces, and AI numbers come only from official provider APIs, requests that pass through Mac Observer, or files you import. Anything that can't be measured is shown as **N/A**, **Unsupported** or **Not present** — never simulated.

---

## Features

| Area | What you get |
|---|---|
| **System** | CPU (total, user/system/idle, per-core, load average, top processes) · Memory (used/available, app/wired/compressed, macOS memory-pressure level, swap) · GPU (model, cores, utilisation, memory in use) · Disk (capacity, read/write throughput) · Battery (%, charging, health, cycle count, time remaining, temperature) · Network (download/upload rate, totals) · Temperatures (IOHID sensors) · Fan status |
| **History** | Performance charts for 1 m / 5 m / 15 m / 1 h / 24 h, stored in SQLite with retention, backup and CSV export |
| **AI usage** | 7 providers (OpenAI, Anthropic, Gemini, OpenRouter, DeepSeek, Groq, any OpenAI-compatible endpoint) · per-provider cards with connection status · tokens (input / cached / output) and cost · charts by provider, model, day, month · filters Today / Yesterday / 7 d / 30 d / This month / Custom |
| **Data provenance** | Every AI number is tagged **Provider reported**, **API telemetry**, **Imported** or **Estimated**; sources are de-duplicated per provider and UTC day |
| **Menu bar** | Live title (CPU %, RAM %, AI tokens today — configurable) and a dropdown with system & AI summaries, widget toggle, pause, settings, quit |
| **Floating widget** | Always-on-top, frameless, draggable, resizable; compact (CPU/RAM/battery/tokens) and expanded (charts, network, providers) modes |
| **Alerts** | CPU, memory, memory pressure, low battery, disk full, daily tokens, daily/monthly budget, provider errors, rejected API keys — thresholds, sustain time, cooldown, native notifications |
| **Security** | API keys in the macOS Keychain only; never in SQLite, logs or the web view; prompt/response content is never stored |

## Requirements

* macOS 14 Sonoma or later (Apple Silicon recommended; Intel works without some sensors)
* To build: Xcode Command Line Tools, Rust ≥ 1.80, Node ≥ 20, pnpm ≥ 9

## Install a built app

1. Open `Mac Observer_0.1.0_aarch64.dmg` and drag **Mac Observer** to Applications.
2. The build is **not signed/notarized**, so the first launch needs: right-click the app → **Open** → **Open**
   (or `xattr -dr com.apple.quarantine "/Applications/Mac Observer.app"`).
3. Allow notifications when macOS asks (System Settings → Notifications → Mac Observer).

Closing the window keeps the app running in the menu bar; quit from the menu bar item or ⌘Q.

## Develop

```bash
pnpm install            # pnpm may ask to approve the esbuild build script (already allowed in pnpm-workspace.yaml)
pnpm tauri dev          # run the app with hot reload
```

Checks:

```bash
pnpm typecheck                       # TypeScript
pnpm test                            # frontend unit tests (Vitest)
cd src-tauri && cargo test           # Rust unit tests
cargo test keychain -- --ignored     # writes/reads/deletes one throwaway item in your login Keychain
```

## Build

```bash
pnpm tauri build
```

Outputs:

* `src-tauri/target/release/bundle/macos/Mac Observer.app`
* `src-tauri/target/release/bundle/dmg/Mac Observer_<version>_aarch64.dmg`

Regenerate icons from `src-tauri/icons/icon-1024.png` with `pnpm icons`.

## Setting up AI providers

Settings → **AI Providers**. Keys are saved straight to the Keychain (`service = com.macobserver.app`).

| Provider | Official usage data | What you need |
|---|---|---|
| OpenAI | Usage API + Costs API | **Admin key** (`sk-admin-…`, Organization → Admin keys). A normal key works for connection tests & telemetry. |
| Anthropic | Usage & Cost Analytics API (legacy reports as fallback) | **Admin API key** (`sk-ant-admin…`). Needs a Console organization; data lags a few hours. |
| Gemini | — (no public usage endpoint for API keys) | API key; usage via telemetry/import |
| OpenRouter | Credits used today / month / total, remaining limit | API key |
| DeepSeek | Account balance only | API key |
| Groq | — | API key |
| Custom | — | Base URL incl. `/v1` (http only for localhost), optional key |

Then: **Test connection** (no tokens spent) → pick models → enable the provider. Providers with a usage/credit API sync on the chosen interval (default 60 min; the first sync fetches 30 days).

### Capturing usage from your own code (telemetry)

Mac Observer cannot see token usage of other apps (ChatGPT/Claude desktop apps, browsers). To record usage from your scripts/tools, enable the **local telemetry proxy** (Settings → AI Providers) and point the SDK's base URL at it. Requests are forwarded unchanged; your client keeps sending its own API key; only token counts, model and time are stored.

```python
from openai import OpenAI
client = OpenAI(base_url="http://127.0.0.1:47821/openai/v1")      # streaming: stream_options={"include_usage": True}

import anthropic
client = anthropic.Anthropic(base_url="http://127.0.0.1:47821/anthropic")
```

Prefixes: `/openai/v1`, `/anthropic`, `/gemini`, `/openrouter/v1`, `/deepseek`, `/groq/v1`, `/custom`.
The proxy listens on `127.0.0.1` only and rejects requests whose Host isn't local.

**Send test request** (per provider) sends one ~20-token request and records its usage — useful to verify the pipeline end-to-end. It is billed at normal rates and asks for confirmation.

### Importing usage (CSV / JSON)

AI Usage → **Import**. Columns (case-insensitive; aliases in brackets):

| column | required | notes |
|---|---|---|
| `date` [`timestamp`, `time`, `day`] | yes | `YYYY-MM-DD` (whole UTC day), RFC 3339, `YYYY-MM-DD HH:MM:SS` (UTC) or Unix s/ms |
| `provider` | yes | `openai`, `anthropic`, `gemini`, `openrouter`, `deepseek`, `groq`, `custom` |
| `model` | yes | |
| `input_tokens` [`prompt_tokens`, `input`] | | **uncached** input |
| `output_tokens` [`completion_tokens`, `output`] | | |
| `cached_tokens` [`cache_read_tokens`] | | |
| `cache_write_tokens` | | |
| `requests` | | |
| `cost_usd` [`cost`] | | stored as imported (actual) cost |

JSON: an array of objects with the same keys, or `{"records": [...]}`. Rows are hashed, so importing the same file twice does not double count. **Export AI usage (CSV)** produces this format, so exports can be re-imported.

### How totals and cost are computed

* **De-duplication:** for each provider and UTC day, only the best available source is counted — *provider reported > imported > telemetry*. (Settings → AI Providers → *Usage source* can force one source.)
* **Actual cost** = provider cost APIs (OpenAI, Anthropic, OpenRouter credits) or cost in the response (OpenRouter) or imported cost.
* **Estimated cost** = tokens × your price table (USD per 1 M tokens). Anthropic list prices are pre-filled (from Anthropic's model table cached 2026-11-03 — verify them); other providers start **empty** on purpose: add current prices from their pricing pages. Tokens without a price are reported as *unpriced*, never guessed.
* Model prices match exactly or by prefix (`gpt-4o` matches `gpt-4o-2024-08-06`).

## Where data lives

| What | Location |
|---|---|
| Database | `~/Library/Application Support/com.macobserver.app/mac-observer.db` |
| Logs (7 days) | `~/Library/Logs/com.macobserver.app/` |
| API keys | login Keychain, service `com.macobserver.app` |
| Launch-at-login | a LaunchAgent plist in `~/Library/LaunchAgents/` (only when enabled) |

Retention defaults: system metrics 7 days (10 s resolution), AI records forever, alert history 90 days — all configurable in Settings → History & Storage, where you can also back up the database (`VACUUM INTO`) and export CSV.

## Metric sources (Apple Silicon)

| Metric | Source | Status on the reference Mac (MacBook Air M1) |
|---|---|---|
| CPU total / per core | sysinfo (`host_processor_info`) | ✅ |
| CPU user / system / idle | `host_statistics(HOST_CPU_LOAD_INFO)` | ✅ |
| Memory breakdown | `host_statistics64(HOST_VM_INFO64)` (Activity Monitor definitions) | ✅ |
| Memory pressure | `sysctl kern.memorystatus_vm_pressure_level` | ✅ |
| GPU utilisation / memory | IOKit `IOAccelerator` → `PerformanceStatistics` | ✅ (unified memory: no VRAM total) |
| Disk capacity / I/O | sysinfo (`statfs`, `IOBlockStorageDriver` statistics) | ✅ |
| Battery | IOKit `AppleSmartBattery` | ✅ |
| Network | sysinfo (`getifaddrs`), physical interfaces only | ✅ |
| Temperatures | IOHID sensors via sysinfo | ✅ CPU die & battery; GPU usually not separately exposed → *Unsupported* |
| Fan RPM | — | *Not present* on fanless models; *Unsupported* elsewhere (needs private AppleSMC access, not implemented) |

## Known limitations

* **Fan RPM** is not implemented (private SMC interface; untestable on the fanless reference machine).
* **GPU temperature** is only shown when the Mac exposes a GPU-labelled sensor.
* **Gemini, Groq, DeepSeek** have no public usage API → telemetry/import only. Anthropic/OpenAI official usage needs **admin** keys.
* OpenRouter credit data is stored per UTC day from the "used today" figure; the last minutes before UTC midnight may be missed if no sync runs then.
* Provider buckets are **UTC days**; filters use local time and include a bucket when its midpoint falls inside the range.
* Not signed or notarized; the Keychain may ask once for access when a different build (e.g. dev vs release) reads a key saved by another build.
* History rows are flushed every 60 s; if the process is killed (not quit), up to a minute of history is lost.
* Only a dark theme is designed; Vietnamese translation covers navigation and key labels.

## Project layout

```
src/                     React UI (pages, components, stores, i18n)
src/widget/              floating widget
src-tauri/src/system/    collectors (cpu, memory, disk, battery, network, gpu, temperature, fan), sampler, history
src-tauri/src/ai/        provider adapters, usage parsing, cost, proxy, import, scheduler
src-tauri/src/database/  SQLite, versioned migrations, repository
src-tauri/src/alerts/    rule engine + notifications
src-tauri/src/security/  Keychain + secret redaction
src-tauri/src/commands/  Tauri commands
```

See [ARCHITECTURE.md](ARCHITECTURE.md) and [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md).

No `.env` file is needed: all configuration is in the app and secrets are in the Keychain.
