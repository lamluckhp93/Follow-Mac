import { listen } from "@tauri-apps/api/event";
import { lazy, Suspense, useEffect, useState } from "react";
import { Header, Sidebar, type View } from "./components/Shell";
import { Dashboard } from "./pages/Dashboard";
import { useSettings } from "./store/settings";

const AiUsage = lazy(() => import("./pages/AiUsage"));
const Alerts = lazy(() => import("./pages/Alerts"));
const SettingsPage = lazy(() => import("./pages/Settings"));

const VIEWS: View[] = ["dashboard", "ai", "alerts", "settings"];

export default function App() {
  const defaultView = useSettings((s) => s.settings?.general.defaultView);
  const [view, setView] = useState<View>("dashboard");
  const [settingsTab, setSettingsTab] = useState<{ tab?: string; provider?: string }>({});
  const openSettings = (tab?: string, provider?: string) => {
    setSettingsTab({ tab, provider });
    setView("settings");
  };

  // Honour the configured default view once settings arrive.
  const [appliedDefault, setAppliedDefault] = useState(false);
  useEffect(() => {
    if (!appliedDefault && defaultView) {
      if (VIEWS.includes(defaultView as View)) setView(defaultView as View);
      setAppliedDefault(true);
    }
  }, [defaultView, appliedDefault]);

  // Navigation requests from the menu bar.
  useEffect(() => {
    const un = listen<string>("app://navigate", (e) => {
      if (VIEWS.includes(e.payload as View)) setView(e.payload as View);
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  // ⌘, opens Settings, ⌘1–4 switch views.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.metaKey) return;
      if (e.key === ",") {
        e.preventDefault();
        setSettingsTab({});
        setView("settings");
      }
      const n = Number(e.key);
      if (n >= 1 && n <= 4) {
        e.preventDefault();
        setView(VIEWS[n - 1]);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="app-bg flex h-full flex-col">
      <Header onSettings={() => openSettings()} />
      <div className="flex min-h-0 flex-1">
        <Sidebar view={view} onChange={(v) => (v === "settings" ? openSettings() : setView(v))} />
        <main className="min-w-0 flex-1 overflow-y-auto px-6 py-5">
          <Suspense fallback={<div className="p-6 text-[12px] text-ink-3">Loading…</div>}>
            {view === "dashboard" && <Dashboard />}
            {view === "ai" && <AiUsage onOpenSettings={(provider) => openSettings("ai", provider)} />}
            {view === "alerts" && <Alerts />}
            {view === "settings" && <SettingsPage key={`${settingsTab.tab}:${settingsTab.provider}`} initialTab={settingsTab.tab} initialProvider={settingsTab.provider} />}
          </Suspense>
        </main>
      </div>
    </div>
  );
}
