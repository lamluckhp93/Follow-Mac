import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, Bell, BellRing, OctagonAlert, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { AlertRulesEditor } from "../components/AlertRules";
import { Badge, Button, Card, Empty, Notice } from "../components/ui";
import { api, errorMessage } from "../lib/api";
import { STATUS } from "../lib/colors";
import { fmtDateTime } from "../lib/format";
import type { AlertEvent } from "../lib/types";

export default function Alerts() {
  const [events, setEvents] = useState<AlertEvent[] | null>(null);
  const [msg, setMsg] = useState<{ tone: "good" | "error"; text: string } | null>(null);

  const load = () =>
    api
      .alertEvents(200)
      .then(setEvents)
      .catch((e) => setMsg({ tone: "error", text: errorMessage(e) }));

  useEffect(() => {
    void load();
    const un = listen("alerts://event", () => void load());
    return () => {
      un.then((f) => f());
    };
  }, []);

  const clear = async () => {
    if (!(await ask("Delete the entire alert history? Alert rules are kept.", { title: "Clear alert history", kind: "warning" }))) return;
    try {
      const n = await api.clearAlertEvents();
      setMsg({ tone: "good", text: `Deleted ${n} events.` });
      void load();
    } catch (e) {
      setMsg({ tone: "error", text: errorMessage(e) });
    }
  };

  const test = async () => {
    try {
      const ok = await api.testNotification();
      setMsg(
        ok
          ? { tone: "good", text: "Test notification sent. If nothing appeared, allow notifications for Mac Observer in System Settings → Notifications." }
          : { tone: "error", text: "macOS refused the notification. Check System Settings → Notifications." },
      );
    } catch (e) {
      setMsg({ tone: "error", text: errorMessage(e) });
    }
  };

  return (
    <div className="mx-auto flex max-w-[1500px] flex-col gap-4">
      <div className="flex items-center justify-between">
        <h1 className="flex items-center gap-2 text-[18px] font-semibold tracking-tight text-ink">
          <Bell size={18} className="accent-text" /> Alerts
        </h1>
        <div className="flex gap-2">
          <Button onClick={test}>
            <BellRing size={13} /> Test notification
          </Button>
          <Button variant="danger" onClick={clear} disabled={!events?.length}>
            <Trash2 size={13} /> Clear history
          </Button>
        </div>
      </div>
      {msg && <Notice tone={msg.tone}>{msg.text}</Notice>}
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-[1.25fr_1fr]">
        <Card title="Rules" icon={<AlertTriangle size={15} />} footer="Conditions must hold for the sustain period; cooldown prevents repeated notifications.">
          <AlertRulesEditor />
        </Card>
        <Card title="Recent alerts" icon={<Bell size={15} />}>
          {!events ? null : events.length === 0 ? (
            <Empty icon={<Bell size={22} />} title="No alerts yet">
              Alerts appear here and as macOS notifications when a rule fires.
            </Empty>
          ) : (
            <div className="flex flex-col">
              {events.map((e) => (
                <div key={e.id} className="flex gap-3 border-b border-line py-2 last:border-0">
                  <span className="mt-0.5" style={{ color: e.severity === "critical" ? STATUS.critical : STATUS.warning }}>
                    {e.severity === "critical" ? <OctagonAlert size={15} /> : <AlertTriangle size={15} />}
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center justify-between gap-2">
                      <span className="truncate text-[13px] text-ink">{e.title}</span>
                      <span className="shrink-0 text-[11px] text-ink-3">{fmtDateTime(e.ts)}</span>
                    </div>
                    <div className="selectable text-[12px] text-ink-2">{e.message}</div>
                    <div className="mt-1 flex gap-1.5">
                      <Badge tone={e.severity === "critical" ? "bad" : "warn"}>{e.severity}</Badge>
                      {!e.notified && <Badge title="macOS did not accept the notification">not delivered</Badge>}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </Card>
      </div>
    </div>
  );
}
