# valorant-api.com — assets we use (from user-supplied reference, 2026-09-27)

Base: https://valorant-api.com/v1 — all lookups by UUID. Cache JSON + images locally.

| Need | Endpoint | Field |
|---|---|---|
| Agent head image (card + details) | /agents?isPlayableCharacter=true | `displayIcon` (square head — see agent-head-icon-example.png). NOT fullPortrait/bustPortrait |
| Player card | /playercards/{uuid} | `smallArt` (square), `wideArt` |
| Rank icons + names | /competitivetiers (use latest table) | `tiers[].smallIcon` / `largeIcon`, `tierName`, `color` |
| Maps | /maps | `listViewIcon` (row bg), `splash`, `displayName`; match by `mapUrl` (Riot gives path like /Game/Maps/...) |
| Act/Episode names + dates | /seasons, /seasons/competitive | `displayName`, `startTime`, `endTime`, `parentUuid` |
| Skins (chosen chroma) | /weapons/skinchromas/{chromaUuid} | `fullRender` / `displayIcon`; weapon via /weapons |
| Weapon UUIDs | /weapons | Vandal, Phantom, Operator, Sheriff |
| Game version | /version | `riotClientVersion` (fallback for X-Riot-ClientVersion) |

Note: /agents returns two Sovas unless `isPlayableCharacter=true`.
