import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { api, errorMessage } from "../lib/api";
import type { ProviderView, QuickSummary } from "../lib/types";

interface AiState {
  summary: QuickSummary | null;
  providers: ProviderView[];
  providersError: string | null;
  /** Bumped whenever usage data changes, so analytics views can refetch. */
  usageVersion: number;
  refreshSummary: () => Promise<void>;
  refreshProviders: () => Promise<void>;
  init: () => Promise<void>;
}

let started = false;

export const useAi = create<AiState>((set, get) => ({
  summary: null,
  providers: [],
  providersError: null,
  usageVersion: 0,
  refreshSummary: async () => {
    try {
      set({ summary: await api.aiSummary() });
    } catch {
      /* summary is best-effort */
    }
  },
  refreshProviders: async () => {
    try {
      set({ providers: await api.providers(), providersError: null });
    } catch (e) {
      set({ providersError: errorMessage(e) });
    }
  },
  init: async () => {
    if (started) return;
    started = true;
    await Promise.all([get().refreshSummary(), get().refreshProviders()]);
    await listen("ai://usage-changed", () => {
      set((s) => ({ usageVersion: s.usageVersion + 1 }));
      // The backend recomputes the summary right after emitting; give it a beat.
      setTimeout(() => get().refreshSummary(), 300);
    });
    await listen("ai://providers-changed", () => get().refreshProviders());
    // Day rollover & scheduled syncs.
    setInterval(() => get().refreshSummary(), 60_000);
  },
}));
