# Independent roster study

This study was designed without inspecting existing visual components, screenshots, design-spec geometry, design-study.md or fresh visual files. `../fresh/data.ts` is used solely as the approved pure data adapter. Existing asset URLs and local IBM Plex Sans fonts are reused. No runtime dependencies, timers, polling, background animations, backend changes or production entry changes were added.

## Product judgment

A live tracker answers three immediate questions: who is here, what rank do they currently hold, and what does their recent competitive record look like? Identity, rank and current form therefore lead the page. It should support understanding teammates and opponents without pretending that rank or a five-game sample predicts the result.

Primary sources consulted:

- [Tracker Network product](https://tracker.gg/VALorant/): explicitly presents live-match rank scouting as a use of its companion app.
- [Tracker Network's own community explanation](https://feedback.tracker.gg/t/is-there-any-chance-to-show-enemy-ranks-on-overlay/32906): identifies all-player rank access in the main live window, with enemy availability after agent select.
- [Tracker Network's queue-group explanation](https://feedback.tracker.gg/t/color-next-to-usernames-meaning-current-match/31810): confirms that identifying players queued together is useful live context.
- [Riot's competitive design explanation](https://playvalorant.com/en-us/news/game-updates/04-on-peeker-s-advantage-ranked/): positions teamplay as central to mastery. This supports avoiding fabricated threat scores or confident strength judgments from K/D alone.

These sources support the task priorities, not a claim that this particular layout has been user-tested.

## Three structural options

1. A shared scouting table, with the two teams stacked. Every rank, K/D, win rate and result sequence shares one vertical axis. A deliberate edited set of fields permits ten comfortably spaced players in one window. Chosen.
2. Two team rails with a permanent inspector. Gives excellent focus to one player, but consumes the width needed for useful comparisons across ten people and forces repeated selection for basic facts. Better suited to a larger window.
3. Two team columns of self-contained cards. Each player can carry more context, but labels repeat ten times, numerical fields stop sharing one comparison axis and rank history competes with present rank. Valid if always-visible peak rank is the priority; not selected here.

## Information placement

- Overview: locked agent, name/tag, remembered-name indicator, encounters, current rank/RR, K/D, act win rate, five results, party marks. Identity and current rank lead; recent form follows. All players receive the same columns and no sorting by a synthetic strength score.
- Details: account level, card art, HS%, previous rank, peak act, match history, frequent teammates, encounters and loadout. HS% explains an individual style but is less useful than K/D for a first comparison. Level does not establish competitive strength. Previous/peak rank is historical context and must not visually rival present rank. Card artwork serves identity better in a profile than repeated as ten decorative backgrounds.
- Results keep letters as well as color; latest is rightmost with an underline. Stat scope remains in native tooltips.
- Lobby/pregame use the same row anatomy. Empty screen area is deliberate; there are no fabricated enemy identities or empty card grid. Agent select only shows locked agent heads. Live stages do not scroll.
- Player selection replaces the roster instead of squeezing it behind a drawer. Back and Escape restore the roster; Show more expands from three to eight recent matches. Detail content can scroll inside its main evidence region.

## Review and revision

First pass confirmed all ten players fit at 1000×800 with a clear rank axis and readable 12px minimum labels. Critique identified a remembered-name glyph that was too ambiguous and unresolved data incorrectly presented like missing data. Revised to a proper eye-off SVG with source tooltip and neutral skeletons. Added real local italic font for hidden identities and preserved tag width in the profile header.

Final checks: six scene screenshots at 1000×800, no document overflow, no text under 12px, no broken images, no browser page errors. Player selection, Show more (eight matches) and Escape back were exercised. Final Svelte/TypeScript check passed with zero errors and zero warnings after all refinements. Screenshots were recaptured only after every image completed loading, then visually inspected.

## Scope and limits

This is a Live comparison prototype only. Other navigation pages and a match scoreboard are intentionally not implemented. Recent match rows are evidence displays rather than links to an unbuilt scoreboard. Detail encounters are a summary rather than a complete encounter history. Production integration, backend behavior and real Riot verification are outside this study.
