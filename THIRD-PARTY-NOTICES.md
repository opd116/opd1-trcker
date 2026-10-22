# Third-party notices

OPD1 TRCKER is proprietary (see `LICENSE`). The components below are used under their own licences. Rust crates and npm packages that are compiled or bundled into the app keep the licences published with them in their registries.

## Fonts: Barlow, IBM Plex Sans, Archivo

The UI bundles latin-subset woff2 files of three typefaces, at the weights it uses (500, 600, 700, and 500 italic). Each one is licensed under the SIL Open Font License, Version 1.1 (OFL). The full licence texts, with each font's copyright line, are in `ui/src/assets/fonts/OFL.txt` (added with the UI lane, Phase 2 Task 16a; the path resolves on `main` after the Phase 2 merge).

| Typeface | Licence |
|---|---|
| Barlow | SIL OFL 1.1 |
| IBM Plex Sans | SIL OFL 1.1 |
| Archivo | SIL OFL 1.1 |

The fonts are bundled as latin subsets and are not sold on their own.

## Rust crates added with R189 (tray, launch with Windows, notifications)

Tauri's `tray-icon` feature and two official Tauri plugins, approved by the user on 2026-10-22, plus the crates they pull into the Windows build:

| Crate | Version | Licence |
|---|---|---|
| tauri-plugin-autostart | 2.7.0 | Apache-2.0 OR MIT |
| tauri-plugin-notification | 2.7.0 | Apache-2.0 OR MIT |
| auto-launch | 0.6.0 | MIT |
| notify-rust | 4.18.2 | MIT OR Apache-2.0 |
| tauri-winrt-notification | 0.8.1 | MIT OR Apache-2.0 |
| windows-registry | 0.6.1 | MIT OR Apache-2.0 |
| os_info | 3.15.0 | MIT |
| dirs | 7.0.0 | MIT OR Apache-2.0 |
| rand, rand_chacha, rand_core | 0.9.x | MIT OR Apache-2.0 |
| ppv-lite86 | 0.2.21 | MIT OR Apache-2.0 |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT |
