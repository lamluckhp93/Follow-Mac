import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { api, errorMessage } from "../lib/api";
import type { DeviceInfo, SystemSnapshot } from "../lib/types";

/** Compact live sample kept in memory for sparklines (last ~5 minutes). */
export interface LivePoint {
  ts: number;
  cpu: number | null;
  mem: number | null;
  gpu: number | null;
  rx: number | null;
  tx: number | null;
  read: number | null;
  write: number | null;
}

const LIVE_WINDOW_MS = 5 * 60_000;

interface SystemState {
  device: DeviceInfo | null;
  snapshot: SystemSnapshot | null;
  live: LivePoint[];
  error: string | null;
  init: () => Promise<void>;
}

function toLive(s: SystemSnapshot): LivePoint {
  return {
    ts: s.ts,
    cpu: s.cpu.usage,
    mem: s.memory.usedPercent,
    gpu: s.gpu.usage,
    rx: s.network.rxBps,
    tx: s.network.txBps,
    read: s.disk.readBps,
    write: s.disk.writeBps,
  };
}

let started = false;

export const useSystem = create<SystemState>((set, get) => ({
  device: null,
  snapshot: null,
  live: [],
  error: null,
  init: async () => {
    if (started) return;
    started = true;
    try {
      const [device, snapshot] = await Promise.all([api.deviceInfo(), api.snapshot()]);
      set({ device, snapshot: snapshot.ts ? snapshot : null });
      // Seed sparklines from the backend ring buffer so they are not empty on open.
      const hist = await api.systemHistory(5);
      set({
        live: hist.map((p) => ({
          ts: p.ts,
          cpu: p.cpuUsage,
          mem: p.memUsed != null && p.memTotal ? (p.memUsed / p.memTotal) * 100 : null,
          gpu: p.gpuUsage,
          rx: p.netRxBps,
          tx: p.netTxBps,
          read: p.diskReadBps,
          write: p.diskWriteBps,
        })),
      });
    } catch (e) {
      set({ error: errorMessage(e) });
    }
    await listen<SystemSnapshot>("system://snapshot", (e) => {
      const s = e.payload;
      const cutoff = s.ts - LIVE_WINDOW_MS;
      const prev = get().live;
      const last = prev[prev.length - 1];
      const next = s.monitoring && (!last || s.ts > last.ts) ? [...prev.filter((p) => p.ts >= cutoff), toLive(s)] : prev;
      set({ snapshot: s, live: next, error: null });
    });
  },
}));
