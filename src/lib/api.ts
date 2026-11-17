import { invoke } from "@tauri-apps/api/core";
import type {
  AlertEvent,
  AlertRule,
  Analytics,
  AppInfo,
  ConnectionTest,
  DeviceInfo,
  ImportReport,
  MetricPoint,
  ModelPrice,
  ProviderView,
  ProxyStatus,
  QuickSummary,
  Settings,
  StorageStats,
  SystemSnapshot,
  TestRequestResult,
  UsageRecord,
} from "./types";

/** Normalise a rejected invoke (AppError `{kind,message}` or string) into an Error. */
export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return "Unknown error";
}

const call = <T>(cmd: string, args?: Record<string, unknown>) => invoke<T>(cmd, args);

export const api = {
  // system
  deviceInfo: () => call<DeviceInfo>("get_device_info"),
  snapshot: () => call<SystemSnapshot>("get_snapshot"),
  systemHistory: (minutes: number) => call<MetricPoint[]>("get_system_history", { minutes }),

  // settings
  getSettings: () => call<Settings>("get_settings"),
  updateSettings: (settings: Settings) => call<Settings>("update_settings", { settings }),
  appInfo: () => call<AppInfo>("get_app_info"),
  setWidgetMode: (mode: "compact" | "expanded") => call<void>("set_widget_mode", { mode }),
  showMainWindow: (view?: string) => call<void>("show_main_window", { view }),

  // ai
  providers: () => call<ProviderView[]>("list_providers"),
  saveProviderConfig: (config: {
    id: string;
    enabled: boolean;
    syncIntervalMinutes: number;
    baseUrl: string | null;
    selectedModels: string[];
  }) => call<void>("save_provider_config", { config }),
  setProviderKey: (provider: string, kind: "api" | "admin", secret: string) =>
    call<void>("set_provider_key", { provider, kind, secret }),
  removeProviderKey: (provider: string, kind: "api" | "admin") => call<void>("remove_provider_key", { provider, kind }),
  removeProviderCredentials: (provider: string) => call<void>("remove_provider_credentials", { provider }),
  testProvider: (provider: string) => call<ConnectionTest>("test_provider_connection", { provider }),
  syncProvider: (provider: string) => call<string>("sync_provider", { provider }),
  sendTestRequest: (provider: string, model: string) => call<TestRequestResult>("send_test_request", { provider, model }),
  analytics: (from: number, to: number, filter?: { providers: string[]; models: string[] }) =>
    call<Analytics>("get_ai_analytics", { from, to, filter: filter ?? null }),
  aiSummary: () => call<QuickSummary>("get_ai_summary"),
  recentTelemetry: (limit = 50) => call<UsageRecord[]>("recent_telemetry", { limit }),
  prices: () => call<ModelPrice[]>("list_prices"),
  upsertPrice: (price: ModelPrice) => call<void>("upsert_price", { price }),
  deletePrice: (provider: string, model: string) => call<void>("delete_price", { provider, model }),
  importUsageFile: (path: string) => call<ImportReport>("import_usage_file", { path }),
  proxyStatus: () => call<ProxyStatus>("get_proxy_status"),
  deleteUsageSource: (source: string) => call<number>("delete_usage_source", { source }),

  // alerts
  alertRules: () => call<AlertRule[]>("list_alert_rules"),
  updateAlertRule: (rule: AlertRule) => call<void>("update_alert_rule", { rule }),
  alertEvents: (limit = 100) => call<AlertEvent[]>("list_alert_events", { limit }),
  clearAlertEvents: () => call<number>("clear_alert_events"),
  testNotification: () => call<boolean>("send_test_notification"),

  // storage
  storageStats: () => call<StorageStats>("get_storage_stats"),
  runRetention: () => call<number>("run_retention_now"),
  backupDatabase: (path: string) => call<string>("backup_database", { path }),
  exportSystemCsv: (path: string, days: number) => call<number>("export_system_csv", { path, days }),
  exportAiCsv: (path: string) => call<number>("export_ai_csv", { path }),
};
