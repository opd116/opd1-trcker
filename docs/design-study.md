# OPD1 TRCKER design study — 2026-10-15

The design problem is selecting useful information and making it comfortable to read in a 1000×800 game companion. Novelty, more fields and more controls are not success measures. Lightness takes precedence over decoration.

## Product questions and hierarchy

| Question | Live | Deeper context |
| --- | --- | --- |
| Who is playing? | Identity, distinct tag, locked agent, small real card art | Level, privacy provenance |
| Where are they currently ranked? | Current tier and RR | Previous tier, peak, peak act/dates |
| How have their games gone? | K/D, HS%, act Win%, five results | Recent matches, sample size and exact scopes |
| Do I recognize them or are they together? | Encounter count and party mark | Encounter history and likely/confirmed explanation |

Identity is primary, current rank is secondary, recent form is a single aligned band. No fabricated player rating, recommendation or inferred skill conclusion.

## Research and decisions

- [Tracker Network's official VALORANT app](https://tracker.gg/valorant/app) establishes live ranks, profiles and history as real tracker workflows. Its dense live screenshot is a comparison reference, not a template: the advertising/sidebar and many equally weighted columns are unsuitable for this small quiet companion.
- [Blitz's official VALORANT page](https://blitz.gg/valorant) connects information to agent select, loading and play. This supports stage-aware context, not adding every possible metric to every stage. Its feature screenshots could not be loaded in this research; no visual claim relies on them.
- [Carbon's table guidelines](https://www.carbondesignsystem.com/building-blocks/core/components/data-table/guidelines) support aligned comparison and revealing supplementary information progressively. This does not prescribe a table for this app.

Design judgments: a full ten-row ledger is useful for comparing one metric but turns recognition into column searching. A ten-column matrix gives too little width to names. Two rows of five compress each player into a tall narrow sheet. Two team columns give names room and preserve obvious team membership; their usefulness depends on editing content rather than reproducing ten miniature dashboards.

## Rejected exploration

Three generated directions tried a full ledger, a transposed matrix and two rows of five. All were over-constrained by the mistaken assumption that every legacy field must stay on Live. The user corrected that assumption. These are rejected explorations, not three approved options.

A fourth concept uses compact team summaries and moves historical rank information into Details. Its generated art is too dominant; implementation deliberately uses a small 40px card crop and 24px locked-agent head. This adjustment is made before accepting the implementation target.

## Implementation target

Graphite background #141719, surface #1c2023, off-white #e4e7e8, secondary #a6afb3, sage accent #b5c6b1. IBM Plex Sans, locally bundled, 12px minimum. No blur, shadows, continuous motion, extra runtime dependencies, new data requests or timers. Existing deterministic mock fixtures and data contracts remain the source of truth.

The first implementation used two team columns. After an independently designed alternative and comparative review, the primary Live view is an edited roster with shared Player/Rank/KD/HS%/Win%/Recent columns, two stacked team sections and54px player rows. It uses24px outer margins and a64px application header. Two team summaries remain an interactive comparison option in the external workbench. Details and personal statistics can scroll; Live cannot. The preview is DEV-only and does not enter the released bundle.

## Review log

1. Content review rejected all-field density and novelty as evaluation criteria. Historical rank fields were moved to Details; core scopes and privacy retained.
2. Concept review rejected dominant player art before implementation. Recognition art is small, leaving typography and current rank as the useful focal points.
3. First rendered critique found horizontal overflow in the long-name scene, conflicting Haven/​Lotus context when opening a match, recognition images competing with agents, and excessive session-result marks. Corrected grid minimums, limited the header map to Live, made the locked agent the primary recognition image, and show five session results with an expandable full list.
4. At the user's explicit request, a new agent with no inherited conversation received product constraints and available data only. It did not inspect any prior visual component/screenshot/spec geometry or this document. It considered a full roster, roster+inspector and team cards. Its implemented roster then underwent independent screenshot comparison. This is independent exploration, not a restyle brief that prescribes our existing layout.
5. Comparative review favored its aligned Live roster, but rejected adopting its limited profile as a complete replacement. The full study keeps complete recent-match fields, encounters, teammate identities, loadout images/chromas, match scoreboard and personal stats. Integrating the roster also corrects every hidden-name icon and retains HS% with enough room for12px+ text.
6. Final verification covers47 deterministic scenes at1000×800, navigation, expanded-history return state, privacy and workbench comparison. UI unit tests584 passed, Svelte/TypeScript check has zero diagnostics. Production build remains146.62KB JS /50.04KB gzip and39.09KB CSS /6.47KB gzip: study code is absent from the shipped entry. This proves isolation and absence of added dependencies, not measured native RAM/CPU improvements. Passing tests does not establish user preference.

## Deliverables

- `/design.html?scene=ingame`: external scene/layout workbench; primary aligned roster and team-summary comparison, both wired into the complete mock flow.
- `/design-screen.html?scene=ingame`: native1000×800 app frame without workbench controls.
- `/independent.html?scene=ingame`: the independent agent's original limited Live experiment, retained for reviewing the design process. Its profile is deliberately not the full product.
- `ui/src/independent/DESIGN-NOTES.md`: independent reasoning, sources, omissions and critique.

Production UI/backend remain unchanged. No claim is made about live Riot integration or a new installer.
