# lol-remote

English | [繁體中文](README.zh-TW.md)

Control the League of Legends client from your phone over your local network. When a match is found, the web page on your phone switches automatically, so you can accept or decline, ban, pick your champion, choose summoner spells and switch rune pages from your phone.

The full specification is in [`docs/SPEC.md`](docs/SPEC.md).

## How it works

```
LoL client (LCU, https://127.0.0.1:<port>)
      ▲  REST + WAMP/WSS
lol-remote (single binary, runs in the background)
      └─ HTTP + WebSocket :3000 ──► phone browser (same LAN)
```

lol-remote watches the League client's lockfile. It connects as soon as the client starts, and goes back to idle when the client closes, waiting for the next launch.

## Installation

### Download a prebuilt binary

Get the latest version from [Releases](https://github.com/marjasback2bedge/lol-remote/releases):

- macOS (universal, Intel and Apple Silicon): `lol-remote-macos-universal.tar.gz`
  ```sh
  tar -xzf lol-remote-macos-universal.tar.gz
  xattr -d com.apple.quarantine lol-remote   # the binary is unsigned, so remove the quarantine flag
  ./lol-remote run
  ```
- Windows: `lol-remote-windows.exe` (unsigned; if SmartScreen warns you, choose "More info → Run anyway")

### Build from source

Requires Rust (stable) and Node.js 18+.

```sh
cd web && npm ci && npm run build && cd ..   # build the frontend first (it is embedded into the binary)
cargo build --release -p lol-remote
./target/release/lol-remote run
```

The first run creates a config file and prints a pairing QR code in the terminal.

### Start at login

```sh
./target/release/lol-remote install     # macOS: ~/Library/LaunchAgents/dev.lol-remote.plist
./target/release/lol-remote uninstall
```

- macOS: runs as a LaunchAgent (`RunAtLoad` + `KeepAlive`). Logs are in `~/Library/Application Support/dev.lol-remote.lol-remote/logs/`. While the League client is running, `caffeinate` keeps the computer awake.
- Windows: adds an entry to `HKCU\...\CurrentVersion\Run`. A console window is currently shown while it runs.

`install` records the binary's current absolute path. If you move the binary, run `install` again.

## Pairing

```sh
lol-remote pair
```

Scan the QR code with your phone to open `http://<LAN IP>:3000/?t=<token>`. The token is saved in a cookie, so afterwards you can simply open `http://<computer name>.local:3000`. Keep the page open while queueing; when a match is found it switches to the accept screen automatically.

## Configuration

`lol-remote config path` prints the config file location (on macOS: `~/Library/Application Support/dev.lol-remote.lol-remote/config.toml`).

| Field | Default | Description |
|---|---|---|
| `port` | `3000` | Web server port |
| `lockfile_path` | `""` | Empty = platform default path |
| `auth_token` | random 32 characters | Access token for the phone |
| `auto_accept` | `false` | Can be toggled from the web page |
| `hostname` | `""` | Empty = system name |

## Development

```
crates/lcu        Talks to the LCU: lockfile, REST, WAMP events, champ select logic
crates/server     The lol-remote binary: state machine, API, WebSocket, mDNS
crates/mock-lcu   A fake LCU (self-signed HTTPS + WSS) to test the whole flow without the game
web/              Vite + Preact + TypeScript frontend
```

### Testing with mock-lcu

```sh
# Terminal 1: fake client, writes ./mock-lockfile
cargo run -p mock-lcu
#   commands: lobby | queue (ready check in 3s) | rc | cs (champ select) | game | none | others-decline | mode classic|aram | quit

# Terminal 2
cargo run -p lol-remote -- run --lockfile mock-lockfile

# Frontend hot reload (optional): http://localhost:5173/?t=<token>
cd web && LOLR_BACKEND=http://localhost:3000 npm run dev
```

By default the mock champ select is a full 10-player draft: the other 9 players ban and pick automatically on their turns. After `mode aram`, the next champ select is ARAM instead: champions are assigned directly, you get one reroll, and teammates' rerolls put champions on the bench.

### Checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd web && npm run typecheck
```

CI (`.github/workflows/build.yml`) runs these checks and builds a macOS universal (arm64 + x86_64) binary and a Windows binary.

### Publishing a release

Push a `v*` tag on `main`. CI builds both binaries and creates a GitHub Release with them attached:

```sh
git checkout main && git pull
git tag v0.2.0 && git push origin v0.2.0
```

## Security

- Every `/api/*`, `/ws` and `/assets/lcu/*` request requires the token (`Authorization: Bearer` or the `lolr_token` cookie), compared in constant time.
- The LCU password and port are never sent to the phone. The phone can only call the operations listed in the spec (§7.8); there is no generic LCU proxy, and the image proxy only allows `/lol-game-data/assets/`.
- Traffic on the LAN is plain HTTP, so **use it only on a trusted home network**.

## Known limitations

- The LCU API is unofficial and not guaranteed to be stable; client updates may break features.
- TLS to the LCU uses option B from the spec: certificate verification is skipped for `127.0.0.1` only (the Riot root certificate is not bundled yet).
- The ready check only lasts about 10 seconds, so keep the page open on your phone while queueing. Turn on auto accept if you step away.
- Your phone must be on the same local network as the computer.
- Not yet verified against a real client (the M1–M4 acceptance steps require actually playing).

## Disclaimer

This project is not affiliated with or endorsed by Riot Games. League of Legends is a trademark of Riot Games, Inc. lol-remote only relays the actions you take on your phone; auto accept is its only automation, and champ select never shows other players' names or stats. Check Riot's latest third-party application policy before distributing it publicly.
