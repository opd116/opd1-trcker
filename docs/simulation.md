# Offline Riot Simulation (design)

OPD1 TRCKER already runs its backend logic against mocked Riot services in
`src-tauri/src/pipeline_tests/` and `crates/riot/tests/`. This document names
the scenarios a developer can run without launching VALORANT, and the boundary
between what they prove and what still needs a real client.

## Running

Rust backend scenarios (offline, no network):

```bash
cargo test -p opd1-core -p opd1-riot -p opd1-trcker
```

Named filters:

- `cargo test -p opd1-trcker collection` — skins collection path.
- `cargo test -p opd1-trcker names` — hidden-name memory.
- `cargo test -p opd1-trcker account` — account switch.
- `cargo test -p opd1-riot endpoints` — endpoint decoding.
- `cargo test -p opd1-riot tolerant` — tolerant decoding.

UI checks:

```bash
npm --prefix ui run test
npm --prefix ui run check
npm --prefix ui run build
```

Playwright (requires the UI dev server):

```bash
cd ui && npx playwright test
```

## Scenario coverage

- `lobby`, `agent-select`, `ingame`, `match-end`: `pipeline_tests::status`.
- `account-switch`: `pipeline_tests::account` plus UI account-switch specs.
- `skins-full`, `skins-failure`: `v2::collection` tests.
- `hidden-player`: `pipeline_tests::names_fill` and `name_stability`.

## Isolation

All tests use in-memory SQLite (`:memory:`) or temp dirs, wiremock servers,
and `RiotSession::connect_with_overrides`. No test contacts Riot production.
Missing mock endpoints fail the test rather than falling back.

## What remains unverified without VALORANT

- Real Riot entitlements response shape beyond captured fixtures.
- Real client-version and region parsing from `ShooterGame.log`.
- Live stage transitions timing.
- SmartScreen/installer behavior.
