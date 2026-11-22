# lol-remote — Spec

English | [繁體中文](SPEC.zh-TW.md)

Control the League of Legends client from your phone over the local network: when a match is found, the web page on the LAN switches automatically, and you can accept / decline, pick, ban, and choose summoner spells and rune pages from your phone.

---

## 1. Goals and non-goals

### Goals
- When the user opens the League client, the local server starts working automatically (no manual start).
- When a ready check appears, a phone web page that is already open switches to the accept screen within seconds.
- From the phone's LAN web page, the user can: accept / decline, ban, hover, lock in, choose summoner spells, and switch rune pages.
- Distributed as a single binary, supporting macOS (priority) and Windows.
- First-time phone pairing via QR code, zero configuration afterwards.

### Non-goals (not in v1)
- Auto pick / auto ban or any feature that makes decisions for the player (only the "auto accept" toggle is kept).
- Access from outside the LAN (no Tailscale, no cloud relay).
- Native iOS / Android app, Dynamic Island.
- Push notifications (the user is watching the page while paired, so push is redundant).
- Editing rune page contents (only switching between existing pages).
- Anything in-game.

---

## 2. User flow

```
[Boot] lol-remote daemon runs in the background (idle, almost no resource use)
   │
[Open LoL] daemon detects the lockfile → connects to the LCU → enters Active state
   │
[Queueing] gameflow-phase = Matchmaking
   │
[Match found] gameflow-phase = ReadyCheck
   ├─ If "auto accept" is on → accept immediately
   └─ Otherwise → the web page switches to the Ready Check screen
   │
[Phone] Tap accept on the already open http://<host>.local:3000
   │
[Champ select] gameflow-phase = ChampSelect
   └─ The page shows the countdown, the current action and team status in real time
      → search champion → hover → lock in
      → choose summoner spells D/F, choose rune page
   │
[In game] gameflow-phase = InProgress → page shows "in game"
   │
[Close LoL] lockfile disappears → disconnect → back to Idle
```

---

## 3. Architecture

```
┌──────────── User's computer ────────────┐
│                                          │
│  LoL Client (LCU)                        │
│   https://127.0.0.1:<port>               │
│        ▲ REST + WSS (Basic Auth)         │
│        │                                 │
│  lol-remote (single Rust binary)         │
│   ├─ lcu crate: lockfile/REST/events     │
│   └─ server crate:                       │
│       ├─ axum HTTP + WS  :3000 ──────────┼──► phone browser (LAN)
│       ├─ mDNS announce <host>.local      │
│       └─ embedded frontend static files  │
└──────────────────────────────────────────┘
```

---

## 4. Tech stack

### Backend (Rust, edition 2021, stable)
| Purpose | Crate |
|---|---|
| Async runtime | `tokio` (full) |
| HTTP / WS server | `axum` (with the `ws` feature), `tower-http` (CORS, trace) |
| HTTP client (LCU) | `reqwest` (`rustls-tls`, `json`) |
| LCU WebSocket | `tokio-tungstenite` (`rustls-tls-webpki-roots` or a custom connector) |
| Custom TLS config | `rustls` |
| JSON | `serde`, `serde_json` |
| Config file | `toml`, `directories` |
| Lockfile watching | `notify` |
| mDNS | `mdns-sd` |
| Pairing QR | `qrcode` (terminal output) |
| Embedded frontend | `rust-embed` |
| CLI | `clap` (derive) |
| Logging | `tracing`, `tracing-subscriber` |
| Error handling | `anyhow` (server), `thiserror` (lcu) |
| Random token | `rand` |

### Frontend
- Vite + Preact + TypeScript
- No UI framework, hand-written CSS, mobile-first
- Build output goes to `web/dist` and is bundled into the binary by `rust-embed`

---

## 5. Repo layout

```
lol-remote/
├── Cargo.toml                # workspace
├── crates/
│   ├── lcu/                  # talks to the League client (reusable on its own)
│   │   ├── src/lib.rs
│   │   ├── src/lockfile.rs
│   │   ├── src/client.rs     # REST
│   │   ├── src/events.rs     # WAMP WebSocket subscription
│   │   ├── src/types.rs      # only the fields that are used
│   │   └── src/champ_select.rs
│   ├── server/               # binary: lol-remote
│   │   ├── src/main.rs
│   │   ├── src/config.rs
│   │   ├── src/supervisor.rs # Idle/Active state machine
│   │   ├── src/api.rs        # REST for the phone
│   │   ├── src/ws.rs         # WS for the phone
│   │   ├── src/mdns.rs
│   │   ├── src/pairing.rs    # QR
│   │   ├── src/install.rs    # launchd / Windows start at login
│   │   └── src/assets.rs     # rust-embed + LCU image proxy
│   └── mock-lcu/             # fake LCU server for development (see §12)
├── web/
│   ├── package.json
│   ├── vite.config.ts
│   └── src/
└── README.md
```

---

## 6. `lcu` crate

### 6.1 Lockfile
- Default path (can be overridden in the config file):
  - macOS: `/Applications/League of Legends.app/Contents/LoL/lockfile`
  - Windows: `C:\Riot Games\League of Legends\lockfile`
- Format: `<name>:<pid>:<port>:<password>:<protocol>` (e.g. `LeagueClient:12345:54321:abcXYZ:https`)
- Parsed into `Credentials { pid, port, password, protocol }`.
- Read failures and malformed content must return a clear error, never panic.

### 6.2 REST client
- Base URL: `https://127.0.0.1:{port}`
- Auth: Basic, username `riot`, password from the lockfile.
- TLS: the LCU uses a Riot self-signed certificate. Pick one implementation (A preferred):
  - A. Bundle the Riot root certificate `riotgames.pem` into the rustls root store. Because the certificate's SAN does not include 127.0.0.1, use a custom `ServerCertVerifier` that only verifies the chain and skips the hostname check.
  - B. `danger_accept_invalid_certs(true)`, **only** for the 127.0.0.1 client.
- Methods: `get<T>`, `post<T>`, `patch<T>`, `put<T>`, `delete`, returning `Result<T, LcuError>`; on non-2xx, include the body in the error.

### 6.3 Event subscription (WAMP over WSS)
- Connection: `wss://127.0.0.1:{port}/`, with the same Basic Auth header and TLS config.
- Subscribe by sending `[5, "<eventName>"]`. Required events:
  - `OnJsonApiEvent_lol-gameflow_v1_gameflow-phase`
  - `OnJsonApiEvent_lol-matchmaking_v1_ready-check`
  - `OnJsonApiEvent_lol-champ-select_v1_session`
- Incoming format: `[8, "<eventName>", { "uri": ..., "eventType": "Create|Update|Delete", "data": ... }]`
- Exposes a `tokio::sync::broadcast::Receiver<LcuEvent>`, where `LcuEvent` is an enum:
  - `GameflowPhase(Phase)`
  - `ReadyCheck(Option<ReadyCheck>)`
  - `ChampSelect(Option<ChampSelectSession>)` (None on Delete)
- On disconnect the stream ends; the supervisor decides whether to reconnect.

### 6.4 Endpoints used
| Feature | Method | Path |
|---|---|---|
| Current phase | GET | `/lol-gameflow/v1/gameflow-phase` |
| Summoner info | GET | `/lol-summoner/v1/current-summoner` |
| Ready check state | GET | `/lol-matchmaking/v1/ready-check` |
| Accept | POST | `/lol-matchmaking/v1/ready-check/accept` |
| Decline | POST | `/lol-matchmaking/v1/ready-check/decline` |
| Champ select session | GET | `/lol-champ-select/v1/session` |
| Pickable champions | GET | `/lol-champ-select/v1/pickable-champion-ids` |
| Bannable champions | GET | `/lol-champ-select/v1/bannable-champion-ids` |
| Hover / Lock | PATCH | `/lol-champ-select/v1/session/actions/{actionId}` body `{ "championId": n, "completed": bool }` |
| Summoner spells | PATCH | `/lol-champ-select/v1/session/my-selection` body `{ "spell1Id": n, "spell2Id": n }` |
| Bench swap (ARAM) | POST | `/lol-champ-select/v1/session/bench/swap/{championId}` |
| Reroll (ARAM) | POST | `/lol-champ-select/v1/session/my-selection/reroll` |
| Game mode | GET | `/lol-gameflow/v1/session` (reads `map.gameMode`, fetched once per champ select) |
| Rune pages | GET | `/lol-perks/v1/pages` |
| Current rune page | GET / PUT | `/lol-perks/v1/currentpage` (PUT body is the page id as a number) |
| Champion list | GET | `/lol-game-data/assets/v1/champion-summary.json` |
| Champion icon | GET | `/lol-game-data/assets/v1/champion-icons/{id}.png` |
| Summoner spell list | GET | `/lol-game-data/assets/v1/summoner-spells.json` |
| Spell icon | GET | from `iconPath` in summoner-spells.json |

> Note: the LCU is an unofficial API and fields may change between versions. `types.rs` only declares the fields that are used; everything else is `#[serde(default)]` or ignored as unknown. Do not use `deny_unknown_fields`.

### 6.5 Phase enum
`None, Lobby, Matchmaking, CheckedIntoTournament, ReadyCheck, ChampSelect, GameStart, FailedToLaunch, InProgress, Reconnect, WaitingForStats, PreEndOfGame, EndOfGame, TerminatedInError, Unknown(String)`

### 6.6 Champ select logic (`champ_select.rs`)
- Read `localPlayerCellId` from the session.
- `actions` is a 2D array; flatten it and find actions with `actorCellId == localPlayerCellId`.
- "My turn" = that action has `isInProgress == true && completed == false`.
- Provides:
  - `current_action(&session) -> Option<MyAction { id, kind: Pick|Ban, champion_id, completed }>`
  - `hover(action_id, champion_id)` → PATCH `completed: false`
  - `lock(action_id, champion_id)` → PATCH `completed: true`
- Timer: `timer.adjustedTimeLeftInPhase` (ms) and `timer.phase` from the session, passed through to the frontend.
- Team display: `cellId, championId, championPickIntent, assignedPosition, spell1Id, spell2Id` from `myTeam[]`; only `championId` from `theirTeam[]`. **Never show other players' names or stats** (follows Riot's champ select anonymity policy).

---

## 7. `server` crate

### 7.1 CLI
```
lol-remote run          # run in the foreground (default)
lol-remote pair         # print the pairing QR code and URL in the terminal
lol-remote install      # install start at login
lol-remote uninstall    # remove start at login
lol-remote config path  # print the config file path
```

### 7.2 Config file
- Location: the config dir of `directories::ProjectDirs("dev", "lol-remote", "lol-remote")`, file name `config.toml`.
- Generated automatically on first run:
```toml
port = 3000
lockfile_path = ""          # empty = use the platform default
auth_token = "<random 32 characters>"
auto_accept = false
hostname = ""               # empty = use the system hostname
```
- `auto_accept` can be toggled from the web page and is written back to the file.

### 7.3 Supervisor state machine
```
Idle ──(lockfile appears and parses)──► Connecting
Connecting ──(REST succeeds + WS subscription succeeds)──► Active
Connecting ──(failure)──► retry after 2s (the API isn't ready right after the client starts; up to 60s)
Active ──(WS disconnects / lockfile disappears)──► Idle
```
- While Idle: watch the lockfile's directory with `notify`; if platform events are unreliable, fall back to polling every 3 seconds.
- The HTTP server keeps running while Idle; the frontend shows "waiting for the League client".
- On entering Active: fetch gameflow-phase, champion-summary, summoner-spells and perks pages once and cache them in memory.
- macOS: while Active, spawn `caffeinate -i -w <lol_pid>` to prevent sleep, and kill it when leaving Active.

### 7.4 Ready check handling
- When phase = `ReadyCheck` is received (or a ready-check event with `state == "InProgress"` and `playerResponse == "None"`):
  - `auto_accept == true` → POST accept immediately.
  - Otherwise → only update the state; the web page shows the Ready Check screen.
- Auto accept happens only once per ready check (deduplicated by phase transitions).

### 7.5 mDNS
- Announce `_http._tcp` with `mdns-sd`, instance name `lol-remote`, hostname = system hostname + `.local`.
- macOS already resolves `<hostname>.local` itself; the mDNS announcement is a supplement (needed on Windows).

### 7.6 Pairing
- On `lol-remote pair` and on first run, print in the terminal:
  - URLs: `http://<host>.local:3000/?t=<token>` and `http://<LAN IP>:3000/?t=<token>`
  - A QR code (containing the IP URL above)

### 7.7 Security
- Binds to `0.0.0.0:<port>`, but every `/api/*` and `/ws` request must present the token:
  - Header `Authorization: Bearer <token>`, or
  - Cookie `lolr_token` (written when the frontend is first opened with `?t=`; the token is then removed from the URL with `history.replaceState`).
- Token comparison is constant-time.
- Never send the LCU password / port to the frontend.
- The LCU operations the frontend can trigger are limited to the API listed below; **there is no generic LCU proxy**.
- The image proxy only allows the `/lol-game-data/assets/` prefix.

### 7.8 HTTP API for the phone
All responses are JSON; error format `{ "error": "message" }`.

| Method | Path | Description |
|---|---|---|
| GET | `/api/state` | Full state snapshot (see §8) |
| POST | `/api/ready-check/accept` | Accept |
| POST | `/api/ready-check/decline` | Decline |
| GET | `/api/champions` | `[{ id, name, alias }]` (with pickable / bannable flags) |
| POST | `/api/champ-select/hover` | body `{ championId }` |
| POST | `/api/champ-select/lock` | body `{ championId }` |
| GET | `/api/spells` | `[{ id, name, iconUrl }]` (only spells usable in the current champ select's mode; Summoner's Rift when not in champ select) |
| POST | `/api/champ-select/spells` | body `{ spell1Id, spell2Id }` |
| POST | `/api/champ-select/bench-swap` | body `{ championId }` |
| POST | `/api/champ-select/reroll` | |
| GET | `/api/perks/pages` | `[{ id, name, isCurrent, isEditable }]` |
| POST | `/api/perks/current` | body `{ pageId }` |
| GET / PUT | `/api/settings` | `{ autoAccept: bool }` |
| GET | `/assets/lcu/*path` | Proxies LCU images, adds `Cache-Control: max-age=86400` |
| GET | `/*` | Frontend static files (SPA fallback to index.html) |

### 7.9 WebSocket for the phone `/ws`
- Server → client, JSON, each message has a `type`:
  - `{ "type": "state", "data": State }`: the full snapshot, sent on connect and on every change (simplicity first; no diffs in v1).
- Client → server: not needed in v1 (all actions go through REST).
- Ping every 20 seconds.

---

## 8. State model (shared by frontend and backend)

```ts
type State = {
  connection: "idle" | "connecting" | "active";
  phase: string;                       // gameflow phase
  autoAccept: boolean;
  readyCheck: null | {
    state: string;                     // InProgress, ...
    playerResponse: string;            // None | Accepted | Declined
    timer: number;                     // seconds
  };
  champSelect: null | {
    gameMode: string;                  // CLASSIC, ARAM, CHERRY, ...; empty string if unknown
    timerMs: number;
    timerPhase: string;                // BAN_PICK, FINALIZATION, ...
    receivedAt: number;                // server timestamp; the frontend counts down locally
    myAction: null | {
      kind: "pick" | "ban";
      championId: number;              // 0 = none selected
      completed: boolean;
      isMyTurn: boolean;
    };
    mySelection: { championId: number; spell1Id: number; spell2Id: number; position: string };
    myTeam: Array<{ cellId: number; championId: number; pickIntent: number; position: string; isMe: boolean }>;
    theirTeam: Array<{ championId: number }>;
    bans: { myTeam: number[]; theirTeam: number[] };
    bench: null | number[];            // bench champions; null when the mode has no bench
    rerollsRemaining: null | number;   // null when rerolling isn't allowed
  };
};
```
The `server` crate defines matching Rust structs with `#[serde(rename_all = "camelCase")]`.

---

## 9. Frontend (`web/`)

### 9.1 Screens
Switches automatically based on `state.connection` and `state.phase`; no routing needed:

1. **Waiting** (idle / connecting): "Waiting for the League client to start".
2. **Lobby / queueing**: shows the current phase and the `auto accept` toggle.
3. **Ready Check**: full screen with two large buttons, "Accept" (green) / "Decline" (gray), and a countdown bar; after accepting, shows "Accepted, waiting for other players".
4. **Champ select**:
   - Top: countdown in seconds (decremented locally from `timerMs` and `receivedAt`), whether it's a ban or a pick, and whether it's my turn.
   - My team row: 5 portraits + positions; enemy team: 5 portraits (champion only).
   - Champion picker: search box (matches name and alias, supports Chinese names) + portrait grid; unavailable champions are grayed out. Tap = hover, then confirm with the "Lock in" button at the bottom.
   - Summoner spells: two slots (D / F); tapping one opens the spell picker.
   - Rune page: dropdown.
5. **In game**: "Game in progress".

### 9.2 Requirements
- Mobile-first, touch targets at least 48px, dark mode supported (dark by default).
- WebSocket reconnects automatically on disconnect (exponential backoff, capped at 5 seconds); a banner at the top is shown while disconnected.
- Every action button shows a loading state while its request runs and prevents double taps; failures show a toast.
- Images go through `/assets/lcu/...`.
- No service worker and no PWA manifest (v1).

---

## 10. Start at login (`install`)

### macOS
- Write `~/Library/LaunchAgents/dev.lol-remote.plist`:
  - `ProgramArguments`: [absolute path of the binary, "run"]
  - `RunAtLoad: true`, `KeepAlive: true`
  - `StandardOutPath` / `StandardErrorPath` point to the log directory
- Run `launchctl bootstrap gui/$(id -u) <plist>`; `uninstall` uses `bootout` and deletes the file.
- The daemon stays resident and waits for the lockfile in the Idle state, which gives the "starts when LoL opens" behavior.

### Windows
- Add a value `lol-remote` = `"<exe>" run` under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
- Either build a windowless version with `windows_subsystem = "windows"`, or document the console window.

---

## 11. Milestones and acceptance criteria

### M1 — LCU basics
- [ ] Parse the lockfile (with unit tests: normal, missing fields, empty file).
- [ ] REST client can GET `/lol-gameflow/v1/gameflow-phase`.
- [ ] WS subscription to gameflow-phase; the terminal prints phase changes.
- Acceptance: open LoL → enter lobby → queue; the terminal prints `Lobby → Matchmaking` in order.

### M2 — Ready check
- [ ] Supervisor state machine (Idle/Connecting/Active).
- [ ] Ready check detection and deduplication.
- [ ] `auto_accept`.
- Acceptance: when a match is found the terminal prints ReadyCheck, and it accepts automatically when `auto_accept` is on; still works after closing and reopening LoL.

### M3 — LAN web page (accept)
- [ ] axum server, token auth, rust-embed, `/api/state`, `/ws`.
- [ ] Frontend "waiting / lobby / Ready Check" screens.
- [ ] mDNS, `pair` QR code.
- Acceptance: scan the QR code with the phone to open the page; the screen switches automatically on ready check, and tapping accept succeeds.

### M4 — Champ select
- [ ] Champ select parsing, hover / lock, ban.
- [ ] Summoner spells, rune page switching.
- [ ] Image proxy.
- [ ] Frontend champ select screen.
- Acceptance: in normal games (blind pick and draft) and ranked draft, ban, pick, lock, change spells and change rune page can all be done from the phone.

### M5 — Distribution
- [ ] `install` / `uninstall` (macOS required, Windows second).
- [ ] `caffeinate`.
- [ ] GitHub Actions: build macOS (arm64 + x86_64 universal) and Windows binaries; build the frontend before cargo build.
- [ ] README: installation, pairing, known limitations, disclaimer.

---

## 12. Development and testing

- **mock-lcu crate**: a local HTTPS + WSS server (self-signed certificate) that simulates:
  - Writing a fake lockfile
  - Switching gameflow phase on command (e.g. typing `readycheck` or `champselect` on stdin); `mode aram` / `mode classic` switches the mode of the next champ select
  - Answering every endpoint this spec uses; the champ select session uses a fixed fixture JSON
  - `lol-remote` can point at the mock with `--lockfile <path>`, so the whole flow can be tested without the game.
- Fixtures: put real captured session / ready-check JSON in `crates/lcu/tests/fixtures/` (remove player names and puuids when capturing).
- Unit tests: lockfile parsing, `current_action` (pick / ban / not my turn / completed), State conversion.
- `cargo clippy -- -D warnings` and `cargo fmt --check` must pass.

---

## 13. Known limitations and caveats

- The LCU is an unofficial API with no stability guarantee; client updates may break features.
- Riot's developer policy prohibits apps that "make decisions for the player"; this tool only relays the actions the user takes on the phone, and "auto accept" is the only automation. Check Riot's latest third-party application policy before releasing publicly.
- Other players' names or stats must not be shown during champ select.
- The ready check only lasts about 10 seconds, so the phone page must stay open; users should turn on "auto accept" when stepping away.
- The phone must be on the same LAN as the computer.
- The token travels over plain HTTP on the LAN, so this is only suitable for a trusted home network.
