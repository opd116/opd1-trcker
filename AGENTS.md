# AGENTS.md

Guide for coding agents other than Claude Code. The full project rules are in `CLAUDE.md`.

1. **Read `CLAUDE.md` first. It is binding.** It holds the product spec, stack, design rules, hidden-name rules and hard limits. The user-facing requirements (in Arabic) are `docs/requirements-raw.md`; a newer dated user decision there beats an older rule elsewhere.
2. **The build spec is `docs/design-spec.md`.** It transcribes the approved visual design (sizes, states, tokens, tests). If it disagrees with the design file on something the design covers, the design wins and the spec is fixed. Any change to a visual the design covers goes to the user first.
3. Every decision is recorded in `docs/DECISIONS.md` (rulings R1 onward). Read the relevant ruling before changing behaviour it covers.

## Name

The name is `OPD1 TRCKER`, on purpose. Never "correct" the spelling. The Rust packages are `opd1-core`, `opd1-riot` and `opd1-trcker`; the npm workspace is `opd1-trcker-ui`; the repo is `opd1-trcker`.

## Build and test

Lightness (RAM, CPU, bundle size, request count) is priority #1 and the tie-breaker for every decision. The UI's only runtime dependency is `@tauri-apps/api`. In Phase 2 add no runtime dependency and no Rust crate; later ones need the user's OK and must keep the app light.

```bash
# Rust (needs ui/dist to exist, so build the UI first; a new checkout needs `npm ci` once)
npm ci
npm --prefix ui run build
cargo test -p opd1-core -p opd1-riot -p opd1-trcker

# UI
npm --prefix ui run test
npm --prefix ui run check

# UI in a browser with the DEV demo scenes (?scene=...)
npm --prefix ui run dev

# Playwright, Chromium, viewport 1000x800
cd ui && npx playwright test

# Windows installer (NSIS); CI builds it on windows-latest
npx tauri build
```

Linux needs `libwebkit2gtk-4.1-dev libsoup-3.0-dev librsvg2-dev` to build the Tauri crate. Anything that touches the real Riot client can only be verified on the user's Windows PC with VALORANT installed.

## Git

- Every step is committed with a clear conventional message (`feat(core): ...`, `fix(ui): ...`, `docs: ...`) and pushed right away with `git push origin <branch>`.
- Commit explicit paths only, so nothing else that is staged or modified goes in: `git add <paths> && git commit -m "<message>" -- <paths>`. Never `git add -A` or `git add .`.
- Never force-push. If a push is rejected: `git pull --rebase --autostash origin <branch>`, re-run the tests, push again.
- Never commit `.superpowers/` or any `*.zip`.
- Never put an email address, token or other secret in a committed file.
- Watch CI by commit sha, so an older green run cannot answer for the new push:

```bash
sha=$(git rev-parse HEAD); id=""
for i in $(seq 1 30); do id=$(gh run list -w Build -c "$sha" -L 1 --json databaseId -q '.[0].databaseId'); [ -n "$id" ] && break; sleep 10; done
test -n "$id" && gh run watch "$id" --exit-status
```

## Lanes (Phase 2, R98)

Three checkouts run in parallel. Start every shell command in a worktree with an explicit `cd <worktree> &&`.

| Branch | Directory | Owns |
|---|---|---|
| `main` | `/home/project-1` | docs and the release |
| `p2-backend` | `/home/project-1/.worktrees/p2-backend` | `crates/`, `src-tauri/` and CI (`.github/`) |
| `ui-lane` | `/home/project-1/.worktrees/ui` | `ui/` |

Write only the paths your lane owns. `p2-backend` lands on `main` at the end of Task 15 and again after Task 15b, before the release; `ui-lane` merges into `main` in Task 19 (the release task).
