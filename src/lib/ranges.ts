export type RangeKey = "today" | "yesterday" | "7d" | "30d" | "month" | "custom";

const DAY = 86_400_000;

function startOfDay(d: Date): number {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

/** Resolve a range key to [from, to) in epoch ms, using local time. */
export function resolveRange(key: RangeKey, now = new Date(), custom?: { from: string; to: string }): [number, number] {
  const today = startOfDay(now);
  switch (key) {
    case "today":
      return [today, now.getTime() + 1];
    case "yesterday":
      return [today - DAY, today];
    case "7d":
      return [today - 6 * DAY, now.getTime() + 1];
    case "30d":
      return [today - 29 * DAY, now.getTime() + 1];
    case "month":
      return [new Date(now.getFullYear(), now.getMonth(), 1).getTime(), now.getTime() + 1];
    case "custom": {
      if (!custom?.from || !custom?.to) return [today - 6 * DAY, now.getTime() + 1];
      const [fy, fm, fd] = custom.from.split("-").map(Number);
      const [ty, tm, td] = custom.to.split("-").map(Number);
      const from = new Date(fy, fm - 1, fd).getTime();
      const to = new Date(ty, tm - 1, td).getTime() + DAY;
      return from < to ? [from, to] : [to - DAY, from + DAY];
    }
  }
}

export const SYSTEM_RANGES = [
  { key: 1, label: "1m" },
  { key: 5, label: "5m" },
  { key: 15, label: "15m" },
  { key: 60, label: "1h" },
  { key: 1440, label: "24h" },
] as const;
