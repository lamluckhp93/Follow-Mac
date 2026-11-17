import { describe, expect, it } from "vitest";
import { fmtAgo, fmtBytes, fmtPct, fmtRate, fmtStorage, fmtTokens, fmtUsd, NA } from "./format";
import { resolveRange } from "./ranges";

describe("format", () => {
  it("renders N/A for missing values instead of zero", () => {
    expect(fmtPct(null)).toBe(NA);
    expect(fmtBytes(undefined)).toBe(NA);
    expect(fmtRate(null)).toBe(NA);
    expect(fmtTokens(null)).toBe(NA);
    expect(fmtUsd(null)).toBe(NA);
  });

  it("formats sizes and rates", () => {
    expect(fmtBytes(8 * 1024 ** 3)).toBe("8.0 GB");
    expect(fmtStorage(245_107_195_904)).toBe("245.1 GB");
    expect(fmtRate(1_500_000)).toBe("1.5 MB/s");
    expect(fmtRate(512)).toBe("512 B/s");
  });

  it("formats tokens and money", () => {
    expect(fmtTokens(1_234_567)).toBe("1.23M");
    expect(fmtTokens(999)).toBe("999");
    expect(fmtUsd(0.0012)).toBe("$0.0012");
    expect(fmtUsd(1234.5)).toBe("$1,234.50");
  });

  it("relative time", () => {
    expect(fmtAgo(null)).toBe("never");
    expect(fmtAgo(1000, 1000 + 90_000)).toBe("2m ago");
  });
});

describe("ranges", () => {
  const now = new Date(2026, 10, 17, 15, 30); // Nov 17 2026 15:30 local
  it("today starts at local midnight", () => {
    const [from, to] = resolveRange("today", now);
    expect(new Date(from).getHours()).toBe(0);
    expect(to).toBeGreaterThan(now.getTime());
  });
  it("yesterday is exactly one day", () => {
    const [from, to] = resolveRange("yesterday", now);
    expect(to - from).toBe(86_400_000);
  });
  it("this month starts on day 1", () => {
    expect(new Date(resolveRange("month", now)[0]).getDate()).toBe(1);
  });
  it("custom range is inclusive of the end day", () => {
    const [from, to] = resolveRange("custom", now, { from: "2026-11-01", to: "2026-11-03" });
    expect((to - from) / 86_400_000).toBe(3);
  });
});
