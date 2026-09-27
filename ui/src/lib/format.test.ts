import { describe, expect, it } from "vitest";
import {
  actLabel,
  DASH,
  kd,
  level,
  pct,
  peakLabel,
  rr,
  rrDelta,
  span,
  tierName,
  timeAgo,
} from "./format";

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

describe("timeAgo", () => {
  it("formats three hours ago as 3h", () => {
    expect(timeAgo(Date.now() - 3 * HOUR)).toBe("3h");
  });

  it("uses the injected clock", () => {
    const now = 1_790_000_000_000;
    expect(timeAgo(now - 3 * HOUR, now)).toBe("3h");
    expect(timeAgo(now - 45 * MIN, now)).toBe("45m");
    expect(timeAgo(now - 20 * 1000, now)).toBe("1m");
    expect(timeAgo(now - 2 * DAY, now)).toBe("2d");
    expect(timeAgo(now - 15 * DAY, now)).toBe("2w");
    expect(timeAgo(now - 70 * DAY, now)).toBe("2mo");
    expect(timeAgo(now - 400 * DAY, now)).toBe("1y");
  });

  it("never goes negative for a clock skew", () => {
    const now = 1_790_000_000_000;
    expect(timeAgo(now + 5 * MIN, now)).toBe("1m");
  });
});

describe("span", () => {
  it("formats a duration with one short unit", () => {
    expect(span(59 * MIN)).toBe("59m");
    expect(span(23 * HOUR)).toBe("23h");
    expect(span(6 * DAY)).toBe("6d");
    expect(span(13 * 30 * DAY)).toBe("1y");
    expect(span(800 * DAY)).toBe("2y");
  });
});

describe("actLabel", () => {
  it("passes the backend label through", () => {
    expect(actLabel("E8 A3")).toBe("E8 A3");
    expect(actLabel("V25 A6")).toBe("V25 A6");
  });
});

describe("peakLabel", () => {
  it("joins the act label and the time since that act ended", () => {
    expect(peakLabel({ tier: 21, actLabel: "E8 A3", actName: "Episode 8 Act III", sinceActEndMs: 400 * DAY })).toBe(
      "E8 A3 · 1y",
    );
  });

  it("says This Act when the peak is in the current act", () => {
    expect(peakLabel({ tier: 21, actLabel: "V26 A5", actName: "V26 Act V", sinceActEndMs: null })).toBe("This Act");
  });
});

describe("rrDelta", () => {
  it("signs gains with a plus", () => {
    expect(rrDelta(16)).toBe("+16");
  });

  it("signs losses with a true minus sign", () => {
    expect(rrDelta(-19)).toBe("−19");
  });

  it("shows zero without a sign", () => {
    expect(rrDelta(0)).toBe("0");
  });
});

describe("pct", () => {
  it("formats a 0..1 fraction with one decimal", () => {
    expect(pct(0.545)).toBe("54.5%");
    expect(pct(0.5)).toBe("50.0%");
    expect(pct(0)).toBe("0.0%");
    expect(pct(1)).toBe("100%");
  });
});

describe("kd", () => {
  it("formats a ratio with two decimals", () => {
    expect(kd(1.2345)).toBe("1.23");
    expect(kd(14)).toBe("14.00");
  });
});

describe("rr and level", () => {
  it("formats integers as plain numbers", () => {
    expect(rr(47)).toBe("47");
    expect(level(312)).toBe("312");
  });
});

describe("tierName", () => {
  it("title-cases the valorant-api tier names", () => {
    expect(tierName("DIAMOND 2")).toBe("Diamond 2");
    expect(tierName("Ascendant 1")).toBe("Ascendant 1");
  });
});

describe("missing values", () => {
  it("renders null as an em dash in every formatter", () => {
    expect(DASH).toBe("—");
    expect(timeAgo(null)).toBe("—");
    expect(span(null)).toBe("—");
    expect(actLabel(null)).toBe("—");
    expect(peakLabel(null)).toBe("—");
    expect(rrDelta(null)).toBe("—");
    expect(pct(null)).toBe("—");
    expect(kd(null)).toBe("—");
    expect(rr(null)).toBe("—");
    expect(level(null)).toBe("—");
    expect(tierName(null)).toBe("—");
  });

  it("renders non-finite numbers as an em dash, never NaN", () => {
    expect(pct(Number.NaN)).toBe("—");
    expect(kd(Number.POSITIVE_INFINITY)).toBe("—");
    expect(rrDelta(Number.NaN)).toBe("—");
  });
});
