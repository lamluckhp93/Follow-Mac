import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { api, errorMessage } from "../lib/api";
import type { Settings } from "../lib/types";

interface SettingsState {
  settings: Settings | null;
  error: string | null;
  load: () => Promise<void>;
  /** Apply a mutation and persist. Returns an error message on failure. */
  update: (mutate: (s: Settings) => void) => Promise<string | null>;
}

function applyToDocument(s: Settings) {
  const root = document.documentElement;
  root.dataset.accent = s.appearance.accent;
  root.dataset.compact = String(s.appearance.compact);
  root.dataset.animations = s.appearance.animations ? "on" : "off";
  root.lang = s.general.language;
}

export const useSettings = create<SettingsState>((set, get) => ({
  settings: null,
  error: null,
  load: async () => {
    try {
      const s = await api.getSettings();
      applyToDocument(s);
      set({ settings: s, error: null });
    } catch (e) {
      set({ error: errorMessage(e) });
    }
  },
  update: async (mutate) => {
    const cur = get().settings;
    if (!cur) return "settings not loaded";
    const next: Settings = structuredClone(cur);
    mutate(next);
    try {
      const saved = await api.updateSettings(next);
      applyToDocument(saved);
      set({ settings: saved, error: null });
      return null;
    } catch (e) {
      return errorMessage(e);
    }
  },
}));

let subscribed = false;
export function subscribeSettings() {
  if (subscribed) return;
  subscribed = true;
  listen<Settings>("settings://changed", (e) => {
    applyToDocument(e.payload);
    useSettings.setState({ settings: e.payload });
  });
}
