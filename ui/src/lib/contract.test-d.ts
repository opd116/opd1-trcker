// Type tests (Task 16b, R96): the descriptors in contract.shape.ts, written from the Rust goldens, agree
// with types.ts in both directions. Run by vitest's typecheck (vite.config.ts) and by `npm run check`.
import { expectTypeOf, test } from "vitest";
import { displayNameShapes, enums, shapes, type ShapeOf } from "./contract.shape";
import type {
  DisplayName,
  EncounterCount,
  EncounterDetailRow,
  Insight,
  InsightKind,
  Last5Entry,
  LiveState,
  Loadout,
  MatchDetailsView,
  MatchSummary,
  MatchTeamView,
  MyStats,
  PartyMark,
  Peak,
  PlayerCard,
  PlayerDetails,
  Rank,
  RankChange,
  RankSummary,
  RememberedSource,
  Result,
  RrPoint,
  ScoreRow,
  SessionSummary,
  Stage,
  StatRow,
  StreakMark,
  Team,
  TeammateSummary,
  WeaponSkin,
  WinCell,
} from "./types";

type Variant<K extends DisplayName["kind"]> = Extract<DisplayName, { kind: K }>;

/** What a TS type says about each key: may it be `null`, may it be absent. */
type Flags<X> = {
  [K in keyof X]-?: {
    nullable: null extends X[K] ? true : false;
    optional: object extends Pick<X, K> ? true : false;
  };
};

/** What a descriptor says about each key. */
type DescFlags<S> = {
  [K in keyof S]: {
    nullable: "null" extends (S[K] extends { kinds: readonly (infer E)[] } ? E : never) ? true : false;
    optional: S[K] extends { optional: true } ? true : false;
  };
};

/** The JSON kind(s) a TS value type serializes as (task 16b review minor 2). Distributes over unions. */
type KindOf<T> = T extends null
  ? "null"
  : T extends string
    ? "string"
    : T extends number
      ? "number"
      : T extends boolean
        ? "boolean"
        : T extends readonly unknown[]
          ? "array"
          : T extends object
            ? "object"
            : never;

/** What a TS type says each key's JSON kinds are (an absent optional key is not a kind). */
type Kinds<X> = { [K in keyof X]-?: KindOf<Exclude<X[K], undefined>> };

/** What a descriptor says each key's JSON kinds are. */
type DescKinds<S> = { [K in keyof S]: S[K] extends { kinds: readonly (infer E)[] } ? E : never };

test("every descriptor has exactly the keys of its TS type", () => {
  expectTypeOf<keyof typeof shapes.Rank>().toEqualTypeOf<keyof Rank>();
  expectTypeOf<keyof typeof shapes.Peak>().toEqualTypeOf<keyof Peak>();
  expectTypeOf<keyof typeof shapes.Last5Entry>().toEqualTypeOf<keyof Last5Entry>();
  expectTypeOf<keyof typeof shapes.PartyMark>().toEqualTypeOf<keyof PartyMark>();
  expectTypeOf<keyof typeof shapes.EncounterCount>().toEqualTypeOf<keyof EncounterCount>();
  expectTypeOf<keyof typeof shapes.PlayerCard>().toEqualTypeOf<keyof PlayerCard>();
  expectTypeOf<keyof typeof shapes.LiveState>().toEqualTypeOf<keyof LiveState>();
  expectTypeOf<keyof typeof shapes.MatchSummary>().toEqualTypeOf<keyof MatchSummary>();
  expectTypeOf<keyof typeof shapes.WeaponSkin>().toEqualTypeOf<keyof WeaponSkin>();
  expectTypeOf<keyof typeof shapes.Loadout>().toEqualTypeOf<keyof Loadout>();
  expectTypeOf<keyof typeof shapes.EncounterDetailRow>().toEqualTypeOf<keyof EncounterDetailRow>();
  expectTypeOf<keyof typeof shapes.TeammateSummary>().toEqualTypeOf<keyof TeammateSummary>();
  expectTypeOf<keyof typeof shapes.PlayerDetails>().toEqualTypeOf<keyof PlayerDetails>();
  expectTypeOf<keyof typeof shapes.ScoreRow>().toEqualTypeOf<keyof ScoreRow>();
  expectTypeOf<keyof typeof shapes.MatchTeamView>().toEqualTypeOf<keyof MatchTeamView>();
  expectTypeOf<keyof typeof shapes.MatchDetailsView>().toEqualTypeOf<keyof MatchDetailsView>();
  expectTypeOf<keyof typeof shapes.RankChange>().toEqualTypeOf<keyof RankChange>();
  expectTypeOf<keyof typeof shapes.RankSummary>().toEqualTypeOf<keyof RankSummary>();
  expectTypeOf<keyof typeof shapes.RrPoint>().toEqualTypeOf<keyof RrPoint>();
  expectTypeOf<keyof typeof shapes.SessionSummary>().toEqualTypeOf<keyof SessionSummary>();
  expectTypeOf<keyof typeof shapes.StatRow>().toEqualTypeOf<keyof StatRow>();
  expectTypeOf<keyof typeof shapes.WinCell>().toEqualTypeOf<keyof WinCell>();
  expectTypeOf<keyof typeof shapes.Insight>().toEqualTypeOf<keyof Insight>();
  expectTypeOf<keyof typeof shapes.MyStats>().toEqualTypeOf<keyof MyStats>();
});

test("every DisplayName variant has a descriptor with exactly its keys", () => {
  expectTypeOf<keyof typeof displayNameShapes>().toEqualTypeOf<DisplayName["kind"]>();
  expectTypeOf<keyof typeof displayNameShapes.visible>().toEqualTypeOf<keyof Variant<"visible">>();
  expectTypeOf<keyof typeof displayNameShapes.remembered>().toEqualTypeOf<keyof Variant<"remembered">>();
  expectTypeOf<keyof typeof displayNameShapes.hiddenPlayer>().toEqualTypeOf<keyof Variant<"hiddenPlayer">>();
  expectTypeOf<keyof typeof displayNameShapes.hiddenAgent>().toEqualTypeOf<keyof Variant<"hiddenAgent">>();
});

test("nullable and optional agree per key (R23 tri-state, R102, R124)", () => {
  expectTypeOf<DescFlags<typeof shapes.Rank>>().toEqualTypeOf<Flags<Rank>>();
  expectTypeOf<DescFlags<typeof shapes.Peak>>().toEqualTypeOf<Flags<Peak>>();
  expectTypeOf<DescFlags<typeof shapes.Last5Entry>>().toEqualTypeOf<Flags<Last5Entry>>();
  expectTypeOf<DescFlags<typeof shapes.PartyMark>>().toEqualTypeOf<Flags<PartyMark>>();
  expectTypeOf<DescFlags<typeof shapes.EncounterCount>>().toEqualTypeOf<Flags<EncounterCount>>();
  expectTypeOf<DescFlags<typeof shapes.PlayerCard>>().toEqualTypeOf<Flags<PlayerCard>>();
  expectTypeOf<DescFlags<typeof shapes.LiveState>>().toEqualTypeOf<Flags<LiveState>>();
  expectTypeOf<DescFlags<typeof shapes.MatchSummary>>().toEqualTypeOf<Flags<MatchSummary>>();
  expectTypeOf<DescFlags<typeof shapes.WeaponSkin>>().toEqualTypeOf<Flags<WeaponSkin>>();
  expectTypeOf<DescFlags<typeof shapes.Loadout>>().toEqualTypeOf<Flags<Loadout>>();
  expectTypeOf<DescFlags<typeof shapes.EncounterDetailRow>>().toEqualTypeOf<Flags<EncounterDetailRow>>();
  expectTypeOf<DescFlags<typeof shapes.TeammateSummary>>().toEqualTypeOf<Flags<TeammateSummary>>();
  expectTypeOf<DescFlags<typeof shapes.PlayerDetails>>().toEqualTypeOf<Flags<PlayerDetails>>();
  expectTypeOf<DescFlags<typeof shapes.ScoreRow>>().toEqualTypeOf<Flags<ScoreRow>>();
  expectTypeOf<DescFlags<typeof shapes.MatchTeamView>>().toEqualTypeOf<Flags<MatchTeamView>>();
  expectTypeOf<DescFlags<typeof shapes.MatchDetailsView>>().toEqualTypeOf<Flags<MatchDetailsView>>();
  expectTypeOf<DescFlags<typeof shapes.RankChange>>().toEqualTypeOf<Flags<RankChange>>();
  expectTypeOf<DescFlags<typeof shapes.RankSummary>>().toEqualTypeOf<Flags<RankSummary>>();
  expectTypeOf<DescFlags<typeof shapes.RrPoint>>().toEqualTypeOf<Flags<RrPoint>>();
  expectTypeOf<DescFlags<typeof shapes.SessionSummary>>().toEqualTypeOf<Flags<SessionSummary>>();
  expectTypeOf<DescFlags<typeof shapes.StatRow>>().toEqualTypeOf<Flags<StatRow>>();
  expectTypeOf<DescFlags<typeof shapes.WinCell>>().toEqualTypeOf<Flags<WinCell>>();
  expectTypeOf<DescFlags<typeof shapes.Insight>>().toEqualTypeOf<Flags<Insight>>();
  expectTypeOf<DescFlags<typeof shapes.MyStats>>().toEqualTypeOf<Flags<MyStats>>();
  expectTypeOf<DescFlags<typeof displayNameShapes.visible>>().toEqualTypeOf<Flags<Variant<"visible">>>();
  expectTypeOf<DescFlags<typeof displayNameShapes.remembered>>().toEqualTypeOf<Flags<Variant<"remembered">>>();
  expectTypeOf<DescFlags<typeof displayNameShapes.hiddenPlayer>>().toEqualTypeOf<Flags<Variant<"hiddenPlayer">>>();
  expectTypeOf<DescFlags<typeof displayNameShapes.hiddenAgent>>().toEqualTypeOf<Flags<Variant<"hiddenAgent">>>();
});

test("the base kinds agree per key: a number field is a number on the wire, an array an array (16b review minor 2)", () => {
  expectTypeOf<DescKinds<typeof shapes.Rank>>().toEqualTypeOf<Kinds<Rank>>();
  expectTypeOf<DescKinds<typeof shapes.Peak>>().toEqualTypeOf<Kinds<Peak>>();
  expectTypeOf<DescKinds<typeof shapes.Last5Entry>>().toEqualTypeOf<Kinds<Last5Entry>>();
  expectTypeOf<DescKinds<typeof shapes.PartyMark>>().toEqualTypeOf<Kinds<PartyMark>>();
  expectTypeOf<DescKinds<typeof shapes.EncounterCount>>().toEqualTypeOf<Kinds<EncounterCount>>();
  expectTypeOf<DescKinds<typeof shapes.PlayerCard>>().toEqualTypeOf<Kinds<PlayerCard>>();
  expectTypeOf<DescKinds<typeof shapes.LiveState>>().toEqualTypeOf<Kinds<LiveState>>();
  expectTypeOf<DescKinds<typeof shapes.MatchSummary>>().toEqualTypeOf<Kinds<MatchSummary>>();
  expectTypeOf<DescKinds<typeof shapes.WeaponSkin>>().toEqualTypeOf<Kinds<WeaponSkin>>();
  expectTypeOf<DescKinds<typeof shapes.Loadout>>().toEqualTypeOf<Kinds<Loadout>>();
  expectTypeOf<DescKinds<typeof shapes.EncounterDetailRow>>().toEqualTypeOf<Kinds<EncounterDetailRow>>();
  expectTypeOf<DescKinds<typeof shapes.TeammateSummary>>().toEqualTypeOf<Kinds<TeammateSummary>>();
  expectTypeOf<DescKinds<typeof shapes.PlayerDetails>>().toEqualTypeOf<Kinds<PlayerDetails>>();
  expectTypeOf<DescKinds<typeof shapes.ScoreRow>>().toEqualTypeOf<Kinds<ScoreRow>>();
  expectTypeOf<DescKinds<typeof shapes.MatchTeamView>>().toEqualTypeOf<Kinds<MatchTeamView>>();
  expectTypeOf<DescKinds<typeof shapes.MatchDetailsView>>().toEqualTypeOf<Kinds<MatchDetailsView>>();
  expectTypeOf<DescKinds<typeof shapes.RankChange>>().toEqualTypeOf<Kinds<RankChange>>();
  expectTypeOf<DescKinds<typeof shapes.RankSummary>>().toEqualTypeOf<Kinds<RankSummary>>();
  expectTypeOf<DescKinds<typeof shapes.RrPoint>>().toEqualTypeOf<Kinds<RrPoint>>();
  expectTypeOf<DescKinds<typeof shapes.SessionSummary>>().toEqualTypeOf<Kinds<SessionSummary>>();
  expectTypeOf<DescKinds<typeof shapes.StatRow>>().toEqualTypeOf<Kinds<StatRow>>();
  expectTypeOf<DescKinds<typeof shapes.WinCell>>().toEqualTypeOf<Kinds<WinCell>>();
  expectTypeOf<DescKinds<typeof shapes.Insight>>().toEqualTypeOf<Kinds<Insight>>();
  expectTypeOf<DescKinds<typeof shapes.MyStats>>().toEqualTypeOf<Kinds<MyStats>>();
  expectTypeOf<DescKinds<typeof displayNameShapes.visible>>().toEqualTypeOf<Kinds<Variant<"visible">>>();
  expectTypeOf<DescKinds<typeof displayNameShapes.remembered>>().toEqualTypeOf<Kinds<Variant<"remembered">>>();
  expectTypeOf<DescKinds<typeof displayNameShapes.hiddenPlayer>>().toEqualTypeOf<Kinds<Variant<"hiddenPlayer">>>();
  expectTypeOf<DescKinds<typeof displayNameShapes.hiddenAgent>>().toEqualTypeOf<Kinds<Variant<"hiddenAgent">>>();
});

test("the closed string sets are the TS unions", () => {
  expectTypeOf<(typeof enums.stage)[number]>().toEqualTypeOf<Stage>();
  expectTypeOf<(typeof enums.result)[number]>().toEqualTypeOf<Result>();
  expectTypeOf<(typeof enums.team)[number]>().toEqualTypeOf<Team>();
  expectTypeOf<(typeof enums.perspective)[number]>().toEqualTypeOf<MatchDetailsView["perspective"]>();
  expectTypeOf<(typeof enums.rememberedSource)[number]>().toEqualTypeOf<RememberedSource>();
  expectTypeOf<(typeof enums.streakMark)[number]>().toEqualTypeOf<StreakMark>();
  expectTypeOf<(typeof enums.insightKind)[number]>().toEqualTypeOf<InsightKind>();
});

test("the guard can fail: a descriptor missing a key, or with an extra one, does not compile", () => {
  // @ts-expect-error a missing key must fail
  ({ puuid: { kinds: ["string"] } }) satisfies ShapeOf<PlayerCard>;

  // @ts-expect-error an extra key must fail
  ({ tier: { kinds: ["number"] }, rr: { kinds: ["number"] }, extra: { kinds: ["number"] } }) satisfies ShapeOf<Rank>;

  // The flags catch what the key set cannot: `kills` claimed non-null on the wire while TS says nullable.
  type Wrong = Omit<typeof shapes.MatchSummary, "kills"> & { kills: { kinds: "number"[] } };
  expectTypeOf<DescFlags<Wrong>>().not.toEqualTypeOf<Flags<MatchSummary>>();

  // The kinds catch what the flags cannot: `startMs` sent as a string, or `players` as one object, not an array.
  type AsString = Omit<typeof shapes.Last5Entry, "startMs"> & { startMs: { kinds: "string"[] } };
  expectTypeOf<DescKinds<AsString>>().not.toEqualTypeOf<Kinds<Last5Entry>>();
  type AsObject = Omit<typeof shapes.LiveState, "players"> & { players: { kinds: "object"[] } };
  expectTypeOf<DescKinds<AsObject>>().not.toEqualTypeOf<Kinds<LiveState>>();
  // And a TS type that drifts the other way: a result that may be null where the descriptor says it never is.
  expectTypeOf<DescKinds<typeof shapes.Last5Entry>>().not.toEqualTypeOf<Kinds<Omit<Last5Entry, "result"> & { result: Last5Entry["result"] | null }>>();
});
