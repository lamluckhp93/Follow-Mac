// Mirrors of the Rust payloads (serde camelCase). Optional Rust values arrive as `null`.

export type MetricStatus = "ok" | "unavailable" | "unsupported" | "not_present" | "pending";

export interface ProcessInfo {
  pid: number;
  name: string;
  cpu: number;
  memory: number;
}

interface Section {
  status: MetricStatus;
  error: string | null;
}

export interface CpuMetrics extends Section {
  usage: number | null;
  user: number | null;
  system: number | null;
  idle: number | null;
  perCore: number[];
  loadAvg: [number, number, number] | null;
  topProcesses: ProcessInfo[];
  updatedAt: number;
}

export interface MemoryMetrics extends Section {
  total: number | null;
  used: number | null;
  available: number | null;
  app: number | null;
  wired: number | null;
  compressed: number | null;
  cachedFiles: number | null;
  usedPercent: number | null;
  pressureLevel: number | null;
  pressureLabel: string | null;
  swapTotal: number | null;
  swapUsed: number | null;
  updatedAt: number;
}

export interface GpuMetrics extends Section {
  model: string | null;
  cores: number | null;
  usage: number | null;
  memoryUsed: number | null;
  memoryTotal: number | null;
  unifiedMemory: boolean;
  updatedAt: number;
}

export interface DiskMetrics extends Section {
  name: string | null;
  mountPoint: string | null;
  fileSystem: string | null;
  total: number | null;
  used: number | null;
  free: number | null;
  usedPercent: number | null;
  ioStatus: MetricStatus;
  readBps: number | null;
  writeBps: number | null;
  totalRead: number | null;
  totalWritten: number | null;
  updatedAt: number;
}

export interface BatteryMetrics extends Section {
  percent: number | null;
  charging: boolean | null;
  pluggedIn: boolean | null;
  fullyCharged: boolean | null;
  cycleCount: number | null;
  healthPercent: number | null;
  designCapacityMah: number | null;
  fullChargeCapacityMah: number | null;
  timeRemainingMin: number | null;
  temperatureC: number | null;
  updatedAt: number;
}

export interface NetworkMetrics extends Section {
  rxBps: number | null;
  txBps: number | null;
  totalRx: number | null;
  totalTx: number | null;
  primaryInterface: string | null;
  updatedAt: number;
}

export interface TemperatureMetrics extends Section {
  cpuC: number | null;
  gpuC: number | null;
  batteryC: number | null;
  sensors: { label: string; celsius: number }[];
  updatedAt: number;
}

export interface FanMetrics {
  status: MetricStatus;
  note: string | null;
  rpm: number[];
}

export interface SystemSnapshot {
  ts: number;
  monitoring: boolean;
  cpu: CpuMetrics;
  memory: MemoryMetrics;
  gpu: GpuMetrics;
  disk: DiskMetrics;
  battery: BatteryMetrics;
  network: NetworkMetrics;
  temperature: TemperatureMetrics;
  fan: FanMetrics;
}

export interface DeviceInfo {
  deviceName: string | null;
  modelIdentifier: string | null;
  osName: string | null;
  osVersion: string | null;
  osBuild: string | null;
  kernelVersion: string | null;
  chip: string | null;
  architecture: string;
  physicalCores: number | null;
  logicalCores: number | null;
  performanceCores: number | null;
  efficiencyCores: number | null;
  totalMemory: number | null;
  bootTime: number | null;
  fanless: boolean | null;
}

export interface MetricPoint {
  ts: number;
  cpuUsage: number | null;
  cpuUser: number | null;
  cpuSystem: number | null;
  memUsed: number | null;
  memTotal: number | null;
  memPressure: number | null;
  swapUsed: number | null;
  gpuUsage: number | null;
  diskReadBps: number | null;
  diskWriteBps: number | null;
  netRxBps: number | null;
  netTxBps: number | null;
  batteryPercent: number | null;
  batteryCharging: number | null;
  cpuTemp: number | null;
}

// ------------------------------------------------------------------ settings

export interface Settings {
  general: { launchAtLogin: boolean; startMinimized: boolean; defaultView: string; language: "en" | "vi" };
  appearance: { theme: string; accent: "cyan" | "blue" | "purple"; compact: boolean; animations: boolean };
  monitoring: {
    enabled: boolean;
    cpuIntervalMs: number;
    memoryIntervalMs: number;
    networkIntervalMs: number;
    diskIntervalMs: number;
    batteryIntervalMs: number;
    sensorsIntervalMs: number;
    processIntervalMs: number;
    persistIntervalS: number;
  };
  menuBar: { enabled: boolean; showCpu: boolean; showRam: boolean; showTokens: boolean; showStatus: boolean };
  widget: { enabled: boolean; mode: "compact" | "expanded"; opacity: number };
  history: { systemRetentionDays: number; aiRetentionDays: number; alertRetentionDays: number };
  ai: { proxyEnabled: boolean; proxyPort: number; usageSource: "auto" | "provider_reported" | "telemetry" | "imported" };
}

export interface AppInfo {
  name: string;
  version: string;
  dataDir: string;
  logDir: string;
  databasePath: string;
  schemaVersion: number;
  tauriVersion: string;
  autostartEnabled: boolean | null;
}

// ------------------------------------------------------------------ AI

export type ProviderId = "openai" | "anthropic" | "gemini" | "openrouter" | "deepseek" | "groq" | "custom";
export type ConnectionStatus = "connected" | "not_configured" | "error" | "unsupported" | "syncing" | "disabled";

export interface AccountInfo {
  balanceUsd: number | null;
  balanceLabel: string | null;
  creditsUsedUsd: number | null;
  creditLimitUsd: number | null;
  usedTodayUsd: number | null;
  usedMonthUsd: number | null;
  fetchedAt: number;
}

export interface Capabilities {
  usageApi: boolean;
  costApi: boolean;
  accountInfo: boolean;
  requiresAdminKeyForUsage: boolean;
  telemetry: boolean;
  needsBaseUrl: boolean;
  notes: string;
  adminKeyLabel: string | null;
  docsUrl: string;
}

export interface ProviderView {
  id: ProviderId;
  displayName: string;
  enabled: boolean;
  baseUrl: string | null;
  syncIntervalMinutes: number;
  selectedModels: string[];
  lastSyncAt: number | null;
  lastSyncStatus: string | null;
  lastError: string | null;
  account: AccountInfo | null;
  hasApiKey: boolean;
  hasAdminKey: boolean;
  status: ConnectionStatus;
  capabilities: Capabilities;
  proxyBaseUrl: string | null;
}

export interface ConnectionTest {
  ok: boolean;
  message: string;
  models: string[];
  latencyMs: number;
  usageApi: { ok: boolean; message: string } | null;
}

export interface TestRequestResult {
  ok: boolean;
  status: number;
  model: string;
  usage: { input: number; output: number; cached: number; cacheWrite: number; costUsd: number | null } | null;
  message: string;
  latencyMs: number;
}

export interface Totals {
  input: number;
  output: number;
  cached: number;
  cacheWrite: number;
  reasoning: number;
  requests: number;
  total: number;
  actualCost: number;
  estimatedCost: number;
  unpricedTokens: number;
}

export interface Breakdown {
  key: string;
  provider: ProviderId;
  totals: Totals;
  sources: string[];
}

export interface SeriesPoint {
  label: string;
  ts: number;
  tokensByProvider: Partial<Record<ProviderId, number>>;
  input: number;
  output: number;
  cached: number;
  actualCost: number;
  estimatedCost: number;
}

export interface Analytics {
  from: number;
  to: number;
  totals: Totals;
  byProvider: Breakdown[];
  byModel: Breakdown[];
  daily: SeriesPoint[];
  monthly: SeriesPoint[];
  recordCount: number;
  deduplicatedRecords: number;
}

export interface QuickSummary {
  today: Totals;
  month: Totals;
  connectedProviders: number;
  enabledProviders: number;
}

export interface UsageRecord {
  provider: ProviderId;
  model: string;
  source: string;
  bucketStart: number;
  bucketEnd: number;
  inputTokens: number;
  outputTokens: number;
  cachedTokens: number;
  cacheWriteTokens: number;
  reasoningTokens: number;
  requests: number;
  costUsd: number | null;
  costSource: string | null;
  externalId: string;
}

export interface ModelPrice {
  provider: ProviderId;
  model: string;
  inputPrice: number | null;
  outputPrice: number | null;
  cachedInputPrice: number | null;
  cacheWritePrice: number | null;
  priceSource: string;
  updatedAt: number;
}

export interface ImportReport {
  imported: number;
  totalRows: number;
  errors: { row: number; message: string }[];
}

export interface ProxyStatus {
  enabled: boolean;
  running: boolean;
  port: number;
  error: string | null;
}

// ------------------------------------------------------------------ alerts / storage

export interface AlertRule {
  id: string;
  category: "system" | "ai";
  enabled: boolean;
  threshold: number;
  durationS: number;
  cooldownS: number;
}

export interface AlertEvent {
  id: number;
  ruleId: string;
  severity: "info" | "warning" | "critical";
  title: string;
  message: string;
  value: number | null;
  ts: number;
  notified: boolean;
}

export interface StorageStats {
  systemMetricsRows: number;
  usageRows: number;
  costRows: number;
  alertEventRows: number;
  oldestMetricTs: number | null;
  dbBytes: number;
  schemaVersion: number;
}

export interface AppError {
  kind: string;
  message: string;
}
