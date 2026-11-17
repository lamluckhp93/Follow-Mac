import type { ProviderId } from "./types";

/**
 * Categorical provider palette for the dark surface (#0f1526).
 * Colour follows the provider (entity), never its rank, so filters never repaint.
 * Validated with the data-viz palette validator (dark mode, adjacent pairs):
 * lightness band, chroma floor, CVD ΔE ≥ 9.9, normal-vision ΔE ≥ 19.3, contrast ≥ 3:1 — all PASS.
 */
export const PROVIDER_COLORS: Record<ProviderId, string> = {
  openai: "#1aa3bd",
  anthropic: "#d95926",
  gemini: "#9085e9",
  openrouter: "#c98500",
  deepseek: "#d55181",
  groq: "#0ca30c",
  custom: "#3987e5",
};

export const PROVIDER_ORDER: ProviderId[] = ["openai", "anthropic", "gemini", "openrouter", "deepseek", "groq", "custom"];

export const PROVIDER_LABEL: Record<ProviderId, string> = {
  openai: "OpenAI",
  anthropic: "Anthropic",
  gemini: "Gemini",
  openrouter: "OpenRouter",
  deepseek: "DeepSeek",
  groq: "Groq",
  custom: "Custom",
};

/** Two-series charts (input/output, read/write, rx/tx) reuse slots 1 and 3. */
export const SERIES = {
  a: "#1aa3bd",
  b: "#9085e9",
  c: "#3987e5",
};

/** Status palette — reserved; always shown with an icon + text label. */
export const STATUS = {
  good: "#0ca30c",
  warning: "#fab219",
  serious: "#ec835a",
  critical: "#d03b3b",
};

export const GRID = "rgba(148,163,184,0.10)";
export const AXIS = "#6b7591";

/** Map a percentage to a status colour for gauges (identity is still given by the label). */
export function levelColor(pct: number | null | undefined, warn = 75, crit = 90): string {
  if (pct == null) return "#6b7591";
  if (pct >= crit) return STATUS.critical;
  if (pct >= warn) return STATUS.warning;
  return "var(--accent)";
}
