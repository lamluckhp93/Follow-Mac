# MAC OBSERVER — Architecture

## Overview

```
┌──────────────────────────── Tauri 2 process ────────────────────────────┐
│                                                                          │
│  Webviews (React)            Rust core (tokio)                           │
│  ┌──────────────┐  invoke   ┌────────────────────────────────────────┐   │
│  │ main window  │ ────────► │ commands/*  (thin, validate + delegate) │   │
│  │ widget window│ ◄──────── │                                         │   │
│  └──────────────┘  events   │ system::Sampler  ──► ring buffer (1h)   │   │
│        ▲                    │        │            └► batched DB write  │   │
│        │ "system://snapshot"│        ▼                                 │   │
│        └────────────────────┤ alerts::Engine ──► notifications         │   │
│                             │ ai::Scheduler ──► provider adapters ──► │   │
│  Menu bar (tray) ◄──────────┤ ai::proxy (127.0.0.1) ──► telemetry     │   │
│                             │ database (rusqlite, migrations)          │   │
│                             │ security::keychain (macOS Keychain)      │   │
│                             └────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────────────┘
```

## Rust modules (`src-tauri/src`)

| Module | Responsibility |
|---|---|
| `lib.rs` | App builder: plugins, state, tray, windows, background task lifecycle, graceful shutdown |
| `state.rs` | `AppState`: DB handle, settings, ring buffer, HTTP client, cancellation token |
| `system/` | One collector per subsystem (`cpu`, `memory`, `disk`, `battery`, `network`, `gpu`, `temperature`), `iokit.rs` FFI helpers, `sampler.rs` scheduling loop |
| `ai/provider.rs` | `ProviderAdapter` trait, `ProviderKind`, shared types |
| `ai/{openai,anthropic,gemini,openrouter,deepseek,groq,custom}.rs` | One adapter each |
| `ai/usage.rs` | Usage extraction from any response/SSE format; de-dup aggregation |
| `ai/cost.rs` | Pricing lookup + estimation |
| `ai/proxy.rs` | Local telemetry proxy |
| `ai/import.rs` | CSV/JSON normalisation |
| `ai/scheduler.rs` | Periodic usage sync per enabled provider |
| `database/` | Connection, migrations, repository functions |
| `alerts/` | Rules, evaluation engine with cooldown, native notification sender |
| `security/keychain.rs` | Keychain get/set/delete; secrets never leave Rust |
| `commands/` | Tauri commands grouped by domain |
| `tray.rs` | Menu bar title + live menu |

## Data provenance

Every AI number carries a `source`:

| Source | Meaning |
|---|---|
| `provider_reported` | From the provider's official usage/cost API |
| `telemetry` | Parsed from `usage` metadata of a response that passed through the app (proxy or test request) |
| `imported` | User-imported CSV/JSON |
| `estimated` | Cost computed from tokens × configured price (never applies to tokens) |

**De-duplication rule:** totals are computed per *(provider, UTC day)* using the single best source available for that bucket — `provider_reported` > `imported` > `telemetry`. Sources are never summed together for the same bucket, so a request seen by the proxy and later reported by the provider's Usage API is counted once.

**Cost rule:** “Actual cost” is shown only when a provider cost API reported it (or the response itself carried a cost, e.g. OpenRouter). Otherwise the UI shows “Estimated” from the user-editable pricing table, or “N/A” when the model has no price configured.

## System metric honesty

Every metric value is `Option<T>` in Rust and `T | null` in TS. Each card renders one of:
`value` · `N/A` (temporarily unavailable, with error) · `Unsupported` (no API on this hardware) · `Not present` (hardware absent, e.g. fan on MacBook Air).

## Persistence

* `system_metrics`: one row per persistence interval (default 10 s) averaged from samples; batch-inserted every 60 s in one transaction.
* Ring buffer in memory holds the last hour at sample resolution for live charts.
* History queries bucket rows server-side (`GROUP BY ts / bucket`) to return ≤ ~360 points.
* Retention job runs hourly: deletes system metrics older than N days (default 7), alert events older than 90 days.
* `VACUUM INTO` powers backup; export to CSV/JSON.

## Security

* API keys: Keychain only (`service = com.macobserver.app`, `account = <provider>:<api|admin>`). Frontend receives only `configured: bool`.
* No key in SQLite, localStorage, logs, error messages or events. Errors from providers are sanitised before display.
* Proxy binds to `127.0.0.1` only, forwards client headers untouched, never stores bodies, only usage numbers.
* CSP restricts the webview to local assets; all network I/O happens in Rust.

## Background task lifecycle

All loops select on a shared `CancellationToken`. On `RunEvent::ExitRequested` the token is cancelled, the metric write buffer is flushed and the proxy shuts down gracefully.
