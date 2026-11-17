# MAC OBSERVER — Implementation Plan

Status legend: `[x]` done **and verified** · `[~]` implemented, not verified end-to-end · `[!]` limited (reason given) · `[ ]` todo

Last updated: 2026-11-17.

## Environment (surveyed 2026-11-17)

| Item | Value |
|---|---|
| Hardware | MacBook Air (MacBookAir10,1), Apple M1, 8 GB, **fanless** |
| macOS | 14.3.1 (23D60), arm64 |
| Rust / Cargo | 1.100.0 |
| Node / pnpm | 26.13.1 / 12.21.0 |
| Xcode CLT | /Library/Developer/CommandLineTools (clang 15) |
| Tauri | 2.x (crate 2.12.6, CLI 2.12.5); Tauri 3 is alpha and deliberately not used |

## Technology decisions

| Concern | Choice | Why |
|---|---|---|
| SQLite | `rusqlite` (bundled), calls run on the blocking pool | Simple and synchronous; the async runtime is never blocked |
| Migrations | Versioned SQL files embedded with `include_str!`, tracked in `schema_migrations` | Deterministic and testable |
| System metrics | `sysinfo` + IOKit FFI (battery, GPU stats) + `sysctl` + Mach `host_statistics` | Official Apple APIs, no root, no process spawning per tick |
| Rates | Per-device / per-interface counter deltas | Summing totals spiked when a volume or interface appeared (bug found with real data and fixed) |
| Fan RPM | Not implemented: **Not present** on fanless models, **Unsupported** elsewhere | Needs private AppleSMC calls; untestable on this machine |
| Secrets | macOS Keychain (`security-framework`) | Keys never reach SQLite, logs or the frontend |
| Telemetry | Local proxy (`axum`, 127.0.0.1) + in-app test request | Usage metadata is only visible for requests that pass through the app |
| UI | React 19 + TS + Vite 7 + Tailwind 4 + Recharts + Zustand + Lucide | As specified; provider palette validated for colour-vision deficiency |

## Phases

### Phase 1 — Foundation
- [x] Environment survey
- [x] Planning docs (this file, ARCHITECTURE.md, README.md)
- [x] Tauri 2 + React + TS + Vite scaffold, Tailwind, design tokens
- [x] SQLite with versioned migrations (unit test: apply + idempotent)
- [x] Dashboard shell (header, sidebar, cards)
- [x] `pnpm tauri dev` runs (migrations applied, sampler started)

### Phase 2 — System monitoring (real data)
- [x] CPU total + user/system/idle + per-core + top processes
- [x] Memory: used/available/app/wired/compressed, pressure level, swap
- [x] Disk: capacity + read/write throughput
- [x] Battery: %, charging, cycle count (72), health (92%), temperature
- [x] Network: rx/tx rates and totals
- [x] GPU: model, cores, utilisation, in-use memory
- [x] Temperatures (47 IOHID sensors on the reference Mac)
- [x] Fan: reports "Not present" (fanless), never simulated
- [x] Batched persistence to `system_metrics` + range queries (rows verified in the real DB)
- Verified by `tests/system_integration.rs` against the real hardware

### Phase 3 — AI providers
- [x] Provider adapter trait + registry
- [~] OpenAI (connection test, Usage + Costs APIs with admin key): parsers unit-tested; **no live key tested**
- [~] Anthropic (Usage & Cost Analytics API + legacy fallback): parsers tested against the documented example; **no live key tested**
- [~] Gemini (connection test, telemetry): **no live key tested**
- [~] OpenRouter (key credits), DeepSeek (balance), Groq, Custom: parsers tested; **no live key tested**
- [x] Telemetry parsing for every response format incl. SSE streams (unit tests)
- [~] Local telemetry proxy: unit-tested helpers; not exercised end-to-end with a real provider
- [x] CSV/JSON import with normalisation (unit tests)
- [x] De-duplication across sources; cost estimation with editable pricing (unit tests)
- [x] Token analytics charts + filters (rendered with real empty-state data)
- [x] AI Usage interactivity: click provider card / legend / chart bar to filter (backend `AnalyticsFilter`, unit-tested), Esc clears, tokens↔cost toggle, per-provider sync report, onboarding when nothing is configured, configure deep-links to the provider, EN/VI strings. Verified with 16 scripted click checks in headless Chrome (CDP)

### Phase 4 — Desktop integration
- [~] Menu bar (tray title + live dropdown): built and launched; not visually confirmed (no screen-recording permission in the build environment)
- [x] Floating widget compact/expanded (layout verified in a headless render; compact size fixed to 248×168)
- [x] Native notifications (two real alerts fired, `notified = 1`)
- [x] Single instance (`tauri-plugin-single-instance`): a second launch focuses the running app (verified: second process exits, log shows the hand-off)
- [~] Launch at login (tauri-plugin-autostart LaunchAgent): not toggled during testing

### Phase 5 — History, alerts, cost
- [x] Alert rules + engine with sustain/cooldown, alert events log (unit tests + real `cpu_high` / `memory_pressure` events)
- [x] Budgets (daily/monthly), provider error and auth alerts (unit tests)
- [x] Retention, backup (`VACUUM INTO`), CSV export (retention unit-tested)

### Phase 6 — Hardening & release
- [x] Rust tests: 59 unit + 1 hardware integration + 1 Keychain round-trip (opt-in)
- [x] Frontend: typecheck clean, 8 Vitest tests
- [x] Security review: no secrets in code paths, logs, DB or frontend storage; proxy bound to localhost with a Host check; errors redacted
- [x] Performance (release build): backend ~58 MB RSS, stable; CPU avg ~2% of one core, 5–7% peaks during process scans; WebKit ~97 MB
- [x] `pnpm tauri build` → `Mac Observer.app` (9.0 MB) + `.dmg` (4.8 MB), launches successfully
- [x] Documentation (README, ARCHITECTURE)

## Open items
- Live verification with real API keys for each provider (requires the user's keys).
- Visual check of the menu bar item on screen.
- Code signing / notarization (needs an Apple Developer ID).
- One `system_metrics` row recorded before the rate fix holds an impossible disk-read value (≈124 GB/s); it ages out with the 7-day retention or can be deleted manually.
