import { describe, expect, it } from "vitest";
import {
  actDates,
  actLabel,
  actTitle,
  ago,
  count,
  DASH,
  dateLong,
  dateShort,
  durationMin,
  kd2,
  level,
  pct0,
  peakLabel,
  rr,
  rrDelta,
  score,
  span,
  tierName,
  time12,
} from "./format";

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

describe("span", () => {
  it("formats a duration with one short unit", () => {
    expect(span(20 * 1000)).toBe("1m");
    expect(span(45 * MIN)).toBe("45m");
    expect(span(59 * MIN)).toBe("59m");
    expect(span(2 * HOUR)).toBe("2h");
    expect(span(23 * HOUR)).toBe("23h");
    expect(span(DAY)).toBe("1d");
    expect(span(6 * DAY)).toBe("6d");
    expect(span(15 * DAY)).toBe("2w");
    expect(span(70 * DAY)).toBe("2mo");
    expect(span(13 * 30 * DAY)).toBe("1y");
    expect(span(800 * DAY)).toBe("2y");
  });

  it("never goes negative for a clock skew", () => {
    expect(span(-5 * MIN)).toBe("1m");
  });
});

describe("ago", () => {
  it("is span(now - ts) plus ' ago', with an injectable clock", () => {
    const now = 1_790_000_000_000;
    expect(ago(now - 3 * HOUR, now)).toBe("3h ago");
    expect(ago(now - 45 * MIN, now)).toBe("45m ago");
    expect(ago(now - 400 * DAY, now)).toBe("1y ago");
  });

  it("uses the wall clock by default", () => {
    expect(ago(Date.now() - 3 * HOUR)).toBe("3h ago");
  });
});

describe("actLabel and peakLabel", () => {
  it("passes the backend label through", () => {
    expect(actLabel("E8 A3")).toBe("E8 A3");
    expect(actLabel("V25 A6")).toBe("V25 A6");
  });

  const base = {
    tier: 21,
    actLabel: "E8 A3",
    actName: "EPISODE 8 ACT III",
    actStartMs: Date.parse("2024-04-30T00:00:00Z"),
    actEndMs: Date.parse("2024-06-26T00:00:00Z"),
  };

  it("joins the act label and the time since that act ended", () => {
    expect(peakLabel({ ...base, sinceActEndMs: 400 * DAY })).toBe("E8 A3 · 1y");
  });

  it("says This Act when the peak is in the current act", () => {
    expect(peakLabel({ ...base, sinceActEndMs: null })).toBe("This Act");
  });
});

describe("rrDelta", () => {
  it("signs gains with a plus, losses with U+2212, and shows zero bare", () => {
    expect(rrDelta(16)).toBe("+16");
    expect(rrDelta(-19)).toBe("−19");
    expect(rrDelta(-15)).toBe("−15");
    expect(rrDelta(0)).toBe("0");
  });
});

describe("pct0, kd2, count, score", () => {
  it("pct0 rounds a 0..1 fraction to a whole percent", () => {
    expect(pct0(0.639)).toBe("64%");
    expect(pct0(0.5)).toBe("50%");
    expect(pct0(0)).toBe("0%");
    expect(pct0(1)).toBe("100%");
  });

  it("kd2 keeps two decimals", () => {
    expect(kd2(1.2345)).toBe("1.23");
    expect(kd2(14)).toBe("14.00");
    expect(kd2(0.94)).toBe("0.94");
  });

  it("count pluralises", () => {
    expect(count(1, "game")).toBe("1 game");
    expect(count(0, "game")).toBe("0 games");
    expect(count(53, "game")).toBe("53 games");
    expect(count(1, "ranked player")).toBe("1 ranked player");
    expect(count(3, "ranked player")).toBe("3 ranked players");
  });

  it("score joins two numbers with U+2013", () => {
    expect(score(13, 9)).toBe("13–9");
    expect(score(null, 9)).toBe(DASH);
    expect(score(13, null)).toBe(DASH);
  });
});

describe("actDates (UTC, spec §4.3)", () => {
  it("shows one year when the act starts and ends in the same year", () => {
    expect(actDates(Date.UTC(2026, 7, 26), Date.UTC(2026, 9, 21))).toBe("Aug 26 – Oct 20, 2026");
  });

  it("shows both years across new year", () => {
    expect(actDates(Date.UTC(2025, 10, 12), Date.UTC(2026, 0, 7))).toBe("Nov 12, 2025 – Jan 6, 2026");
    expect(actDates(Date.UTC(2024, 9, 22), Date.UTC(2025, 0, 9))).toBe("Oct 22, 2024 – Jan 8, 2025");
  });

  it("takes the end date from actEndMs - 1, so a midnight boundary belongs to the previous day", () => {
    expect(actDates(Date.UTC(2026, 5, 24), Date.UTC(2026, 7, 26))).toBe("Jun 24 – Aug 25, 2026");
    // one ms later is a new day
    expect(actDates(Date.UTC(2026, 5, 24), Date.UTC(2026, 7, 26) + 1)).toBe("Jun 24 – Aug 26, 2026");
  });

  it("uses Sep, never Sept", () => {
    expect(actDates(Date.UTC(2026, 8, 1), Date.UTC(2026, 9, 1))).toBe("Sep 1 – Sep 30, 2026");
  });
});

describe("actTitle", () => {
  it("expands the V-year and Episode forms", () => {
    expect(actTitle("V26 ACT V")).toBe("VALORANT 2026 · Act 5");
    expect(actTitle("V25 ACT VI")).toBe("VALORANT 2025 · Act 6");
    expect(actTitle("V26 ACT I")).toBe("VALORANT 2026 · Act 1");
    expect(actTitle("EPISODE 9 ACT III")).toBe("Episode 9 · Act 3");
    expect(actTitle("EPISODE 10 ACT II")).toBe("Episode 10 · Act 2");
  });

  it("passes unparseable input through", () => {
    expect(actTitle("Whatever")).toBe("Whatever");
    expect(actTitle("V26 ACT XI")).toBe("V26 ACT XI");
    expect(actTitle("")).toBe("");
  });
});

// vite.config.ts pins TZ=UTC, so local time == UTC here.
describe("local dates (en-US, UTC in tests)", () => {
  const t = Date.UTC(2026, 8, 29, 21, 14);

  it("dateLong", () => {
    expect(dateLong(t)).toBe("Sep 29, 2026");
    expect(dateLong(Date.UTC(2025, 0, 8, 12))).toBe("Jan 8, 2025");
  });

  it("dateShort shows the year only when it is not the current year", () => {
    const now = Date.UTC(2026, 8, 29, 21, 59);
    expect(dateShort(Date.UTC(2026, 8, 12, 23, 18), now)).toBe("Sep 12");
    expect(dateShort(Date.UTC(2025, 11, 31, 10), now)).toBe("Dec 31, 2025");
    expect(dateShort(Date.UTC(2026, 8, 12, 23, 18))).not.toBe(DASH);
  });

  it("time12", () => {
    expect(time12(t)).toBe("9:14 PM");
    expect(time12(Date.UTC(2026, 8, 29, 0, 5))).toBe("12:05 AM");
    expect(time12(Date.UTC(2026, 8, 29, 12, 0))).toBe("12:00 PM");
    expect(time12(Date.UTC(2026, 8, 29, 9, 30))).toBe("9:30 AM");
    // never a narrow no-break space
    expect(time12(t)).not.toMatch(/ /);
  });

  it("durationMin", () => {
    expect(durationMin(38 * MIN)).toBe("38 min");
    expect(durationMin(38 * MIN + 31_000)).toBe("39 min");
    expect(durationMin(0)).toBe("0 min");
  });
});

describe("rr, level and tierName", () => {
  it("formats integers as plain numbers", () => {
    expect(rr(47)).toBe("47");
    expect(level(312)).toBe("312");
  });

  it("title-cases the valorant-api tier names", () => {
    expect(tierName("DIAMOND 2")).toBe("Diamond 2");
    expect(tierName("Ascendant 1")).toBe("Ascendant 1");
  });
});

describe("missing values", () => {
  it("every formatter maps null to an em dash", () => {
    expect(DASH).toBe("—");
    expect(span(null)).toBe("—");
    expect(ago(null)).toBe("—");
    expect(actLabel(null)).toBe("—");
    expect(peakLabel(null)).toBe("—");
    expect(rrDelta(null)).toBe("—");
    expect(pct0(null)).toBe("—");
    expect(kd2(null)).toBe("—");
    expect(count(null, "game")).toBe("—");
    expect(score(null, null)).toBe("—");
    expect(rr(null)).toBe("—");
    expect(level(null)).toBe("—");
    expect(tierName(null)).toBe("—");
    expect(actDates(null, null)).toBe("—");
    expect(actDates(1, null)).toBe("—");
    expect(dateLong(null)).toBe("—");
    expect(dateShort(null)).toBe("—");
    expect(time12(null)).toBe("—");
    expect(durationMin(null)).toBe("—");
  });

  it("renders non-finite numbers as an em dash, never NaN", () => {
    expect(pct0(Number.NaN)).toBe("—");
    expect(kd2(Number.POSITIVE_INFINITY)).toBe("—");
    expect(rrDelta(Number.NaN)).toBe("—");
    expect(time12(Number.NaN)).toBe("—");
  });
});
