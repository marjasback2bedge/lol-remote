# lol-remote — Spec

用手機在區網內遠端操作 League of Legends 客戶端：排到隊時內網網頁自動切換，即可在手機上接受 / 拒絕、選角、Ban、選召喚師技能與符文頁。

---

## 1. 目標與非目標

### 目標
- 使用者打開 LoL 客戶端後，本機 server 自動進入運作狀態（不需手動啟動）。
- Ready check 出現時，已開啟的手機網頁在數秒內自動切換到接受畫面。
- 手機透過區網網頁完成：接受 / 拒絕、Ban、Hover、Lock in、召喚師技能、符文頁切換。
- 單一執行檔散布，支援 macOS（優先）與 Windows。
- 首次使用以 QR code 完成手機配對，之後零設定。

### 非目標（v1 不做）
- 自動選角 / 自動 Ban 等替玩家做決策的功能（只保留「自動接受」開關）。
- 對外網路存取（不做 Tailscale、雲端中繼）。
- 原生 iOS / Android App、動態島。
- 推播通知（使用者配對時會盯著網頁，推播是多餘的）。
- 編輯符文頁內容（只切換既有頁面）。
- 遊戲內（in-game）任何功能。

---

## 2. 使用者流程

```
[開機] lol-remote daemon 常駐（閒置，幾乎不耗資源）
   │
[開啟 LoL] daemon 偵測到 lockfile → 連上 LCU → 進入 Active 狀態
   │
[排隊中] gameflow-phase = Matchmaking
   │
[排到] gameflow-phase = ReadyCheck
   ├─ 若「自動接受」開啟 → 直接 accept
   └─ 否則 → 網頁切換到 Ready Check 畫面
   │
[手機] 在已開啟的 http://<host>.local:3000 按接受
   │
[選角] gameflow-phase = ChampSelect
   └─ 網頁即時顯示倒數、目前輪到的 action、隊伍狀態
      → 搜尋英雄 → Hover → Lock in
      → 選召喚師技能 D/F、選符文頁
   │
[進遊戲] gameflow-phase = InProgress → 網頁顯示「遊戲中」
   │
[關閉 LoL] lockfile 消失 → 斷線 → 回到 Idle
```

---

## 3. 架構

```
┌──────────── 使用者電腦 ────────────┐
│                                     │
│  LoL Client (LCU)                   │
│   https://127.0.0.1:<port>          │
│        ▲ REST + WSS (Basic Auth)    │
│        │                            │
│  lol-remote (Rust 單一執行檔)       │
│   ├─ lcu crate: lockfile/REST/事件  │
│   └─ server crate:                  │
│       ├─ axum HTTP + WS  :3000 ─────┼──► 手機瀏覽器（區網）
│       ├─ mDNS 廣播 <host>.local     │
│       └─ 內嵌前端靜態檔             │
└─────────────────────────────────────┘
```

---

## 4. Tech Stack

### 後端（Rust, edition 2021, stable）
| 用途 | Crate |
|---|---|
| Async runtime | `tokio`（full） |
| HTTP / WS server | `axum`（含 `ws` feature）、`tower-http`（CORS、trace） |
| HTTP client（LCU） | `reqwest`（`rustls-tls`、`json`） |
| LCU WebSocket | `tokio-tungstenite`（`rustls-tls-webpki-roots` 或自訂 connector） |
| TLS 自訂設定 | `rustls` |
| JSON | `serde`、`serde_json` |
| 設定檔 | `toml`、`directories` |
| 監看 lockfile | `notify` |
| mDNS | `mdns-sd` |
| 配對 QR | `qrcode`（終端機輸出） |
| 內嵌前端 | `rust-embed` |
| CLI | `clap`（derive） |
| Log | `tracing`、`tracing-subscriber` |
| 錯誤處理 | `anyhow`（server）、`thiserror`（lcu） |
| 隨機 token | `rand` |

### 前端
- Vite + Preact + TypeScript
- 不使用 UI 框架，手寫 CSS，mobile-first
- build 產物放 `web/dist`，由 `rust-embed` 打包進執行檔

---

## 5. Repo 結構

```
lol-remote/
├── Cargo.toml                # workspace
├── crates/
│   ├── lcu/                  # 與 LoL 客戶端溝通（可獨立重用）
│   │   ├── src/lib.rs
│   │   ├── src/lockfile.rs
│   │   ├── src/client.rs     # REST
│   │   ├── src/events.rs     # WAMP WebSocket 訂閱
│   │   ├── src/types.rs      # 只定義用到的欄位
│   │   └── src/champ_select.rs
│   ├── server/               # binary: lol-remote
│   │   ├── src/main.rs
│   │   ├── src/config.rs
│   │   ├── src/supervisor.rs # Idle/Active 狀態機
│   │   ├── src/api.rs        # 給手機的 REST
│   │   ├── src/ws.rs         # 給手機的 WS
│   │   ├── src/mdns.rs
│   │   ├── src/pairing.rs    # QR
│   │   ├── src/install.rs    # launchd / Windows 開機啟動
│   │   └── src/assets.rs     # rust-embed + LCU 圖片 proxy
│   └── mock-lcu/             # 開發用假 LCU server（見 §12）
├── web/
│   ├── package.json
│   ├── vite.config.ts
│   └── src/
└── README.md
```

---

## 6. `lcu` crate

### 6.1 Lockfile
- 預設路徑（可由設定檔覆寫）：
  - macOS：`/Applications/League of Legends.app/Contents/LoL/lockfile`
  - Windows：`C:\Riot Games\League of Legends\lockfile`
- 格式：`<name>:<pid>:<port>:<password>:<protocol>`（例：`LeagueClient:12345:54321:abcXYZ:https`）
- 解析為 `Credentials { pid, port, password, protocol }`。
- 讀檔失敗、格式錯誤要回傳明確錯誤，不 panic。

### 6.2 REST client
- Base URL：`https://127.0.0.1:{port}`
- Auth：Basic，帳號 `riot`，密碼為 lockfile 的 password。
- TLS：LCU 使用 Riot 自簽憑證。實作方式二選一（優先 A）：
  - A. 內建 Riot 根憑證 `riotgames.pem` 加入 rustls root store，並因憑證 SAN 不含 127.0.0.1，自訂 `ServerCertVerifier` 僅驗證鏈、略過主機名稱。
  - B. `danger_accept_invalid_certs(true)`，**只能**用於 127.0.0.1 的 client。
- 方法：`get<T>`, `post<T>`, `patch<T>`, `put<T>`, `delete`，回傳 `Result<T, LcuError>`；非 2xx 時把 body 帶入錯誤。

### 6.3 事件訂閱（WAMP over WSS）
- 連線：`wss://127.0.0.1:{port}/`，同樣帶 Basic Auth header 與相同 TLS 設定。
- 訂閱：送出 `[5, "<eventName>"]`，需要的事件：
  - `OnJsonApiEvent_lol-gameflow_v1_gameflow-phase`
  - `OnJsonApiEvent_lol-matchmaking_v1_ready-check`
  - `OnJsonApiEvent_lol-champ-select_v1_session`
- 收到格式：`[8, "<eventName>", { "uri": ..., "eventType": "Create|Update|Delete", "data": ... }]`
- 對外提供 `tokio::sync::broadcast::Receiver<LcuEvent>`，`LcuEvent` 為 enum：
  - `GameflowPhase(Phase)`
  - `ReadyCheck(Option<ReadyCheck>)`
  - `ChampSelect(Option<ChampSelectSession>)`（Delete 時為 None）
- 斷線時結束 stream，由 supervisor 決定重連。

### 6.4 使用的 Endpoints
| 功能 | Method | Path |
|---|---|---|
| 目前階段 | GET | `/lol-gameflow/v1/gameflow-phase` |
| 召喚師資訊 | GET | `/lol-summoner/v1/current-summoner` |
| Ready check 狀態 | GET | `/lol-matchmaking/v1/ready-check` |
| 接受 | POST | `/lol-matchmaking/v1/ready-check/accept` |
| 拒絕 | POST | `/lol-matchmaking/v1/ready-check/decline` |
| 選角 session | GET | `/lol-champ-select/v1/session` |
| 可選英雄 | GET | `/lol-champ-select/v1/pickable-champion-ids` |
| 可 Ban 英雄 | GET | `/lol-champ-select/v1/bannable-champion-ids` |
| Hover / Lock | PATCH | `/lol-champ-select/v1/session/actions/{actionId}` body `{ "championId": n, "completed": bool }` |
| 召喚師技能 | PATCH | `/lol-champ-select/v1/session/my-selection` body `{ "spell1Id": n, "spell2Id": n }` |
| 替補席交換（ARAM） | POST | `/lol-champ-select/v1/session/bench/swap/{championId}` |
| 重骰（ARAM） | POST | `/lol-champ-select/v1/session/my-selection/reroll` |
| 遊戲模式 | GET | `/lol-gameflow/v1/session`（讀 `map.gameMode`，每次選角抓一次） |
| 符文頁列表 | GET | `/lol-perks/v1/pages` |
| 目前符文頁 | GET / PUT | `/lol-perks/v1/currentpage`（PUT body 為 page id 數字） |
| 英雄清單 | GET | `/lol-game-data/assets/v1/champion-summary.json` |
| 英雄頭像 | GET | `/lol-game-data/assets/v1/champion-icons/{id}.png` |
| 召喚師技能清單 | GET | `/lol-game-data/assets/v1/summoner-spells.json` |
| 技能圖示 | GET | 依 summoner-spells.json 的 `iconPath` |

> 注意：LCU 為非官方 API，欄位可能隨版本變動。`types.rs` 只定義用到的欄位，其餘 `#[serde(default)]` 或忽略未知欄位；不要 `deny_unknown_fields`。

### 6.5 Phase enum
`None, Lobby, Matchmaking, CheckedIntoTournament, ReadyCheck, ChampSelect, GameStart, FailedToLaunch, InProgress, Reconnect, WaitingForStats, PreEndOfGame, EndOfGame, TerminatedInError, Unknown(String)`

### 6.6 選角邏輯（`champ_select.rs`）
- 從 session 取 `localPlayerCellId`。
- `actions` 是二維陣列，攤平後找 `actorCellId == localPlayerCellId` 的 action。
- 「目前輪到我」= 該 action `isInProgress == true && completed == false`。
- 提供：
  - `current_action(&session) -> Option<MyAction { id, kind: Pick|Ban, champion_id, completed }>`
  - `hover(action_id, champion_id)` → PATCH `completed: false`
  - `lock(action_id, champion_id)` → PATCH `completed: true`
- 計時器：session 內 `timer.adjustedTimeLeftInPhase`（ms）與 `timer.phase`，轉給前端顯示。
- 隊伍顯示：`myTeam[]` 的 `cellId, championId, championPickIntent, assignedPosition, spell1Id, spell2Id`；`theirTeam[]` 只顯示 `championId`。**不得顯示其他玩家名稱或戰績**（遵守 Riot 選角匿名政策）。

---

## 7. `server` crate

### 7.1 CLI
```
lol-remote run          # 前景執行（預設）
lol-remote pair         # 在終端機印出配對 QR code 與網址
lol-remote install      # 安裝開機自動啟動
lol-remote uninstall    # 移除開機自動啟動
lol-remote config path  # 印出設定檔路徑
```

### 7.2 設定檔
- 位置：`directories::ProjectDirs("dev", "lol-remote", "lol-remote")` 的 config dir，檔名 `config.toml`。
- 首次啟動自動產生：
```toml
port = 3000
lockfile_path = ""          # 空字串 = 使用平台預設
auth_token = "<隨機 32 字元>"
auto_accept = false
hostname = ""               # 空字串 = 使用系統 hostname
```
- `auto_accept` 可由網頁切換並寫回檔案。

### 7.3 Supervisor 狀態機
```
Idle ──(lockfile 出現且可解析)──► Connecting
Connecting ──(REST 成功 + WS 訂閱成功)──► Active
Connecting ──(失敗)──► 等待 2s 重試（客戶端剛啟動時 API 尚未就緒，最多 60s）
Active ──(WS 斷線 / lockfile 消失)──► Idle
```
- Idle 時：用 `notify` 監看 lockfile 所在目錄；若平台事件不可靠，fallback 每 3 秒輪詢。
- HTTP server 在 Idle 時也保持運作，前端顯示「等待 LoL 客戶端」。
- 進入 Active 時：抓一次 gameflow-phase、champion-summary、summoner-spells、perks pages 快取在記憶體。
- macOS：Active 期間 spawn `caffeinate -i -w <lol_pid>` 防止睡眠，離開 Active 時 kill。

### 7.4 Ready check 處理
- 收到 phase = `ReadyCheck`（或 ready-check 事件 `state == "InProgress"` 且 `playerResponse == "None"`）時：
  - `auto_accept == true` → 立即 POST accept。
  - 否則 → 只更新 state，由網頁顯示 Ready Check 畫面。
- 同一次 ready check 只自動接受一次（用 phase 轉換去重）。

### 7.5 mDNS
- 以 `mdns-sd` 廣播 `_http._tcp`，instance name `lol-remote`，hostname 使用系統 hostname + `.local`。
- macOS 系統本身已提供 `<hostname>.local` 解析，mDNS 廣播為輔助（Windows 需要）。

### 7.6 配對
- `lol-remote pair` 與首次啟動時，在終端機印出：
  - 網址：`http://<host>.local:3000/?t=<token>` 與 `http://<LAN IP>:3000/?t=<token>`
  - 一個 QR code（內容為上述 IP 網址）

### 7.7 安全
- 只綁定 `0.0.0.0:<port>`，但所有 `/api/*` 與 `/ws` 需驗證 token：
  - Header `Authorization: Bearer <token>`，或
  - Cookie `lolr_token`（前端首次以 `?t=` 進入時寫入 cookie，並以 `history.replaceState` 移除 URL 中的 token）。
- token 比對使用 constant-time 比較。
- 永遠不把 LCU 的 password / port 傳給前端。
- 前端可呼叫的 LCU 操作限定為下方 API 列出的項目，**不提供任意 LCU proxy**。
- 圖片 proxy 只允許 `/lol-game-data/assets/` 前綴。

### 7.8 給手機的 HTTP API
所有回應 JSON；錯誤格式 `{ "error": "message" }`。

| Method | Path | 說明 |
|---|---|---|
| GET | `/api/state` | 完整狀態快照（見 §8） |
| POST | `/api/ready-check/accept` | 接受 |
| POST | `/api/ready-check/decline` | 拒絕 |
| GET | `/api/champions` | `[{ id, name, alias }]`（含 pickable / bannable 標記） |
| POST | `/api/champ-select/hover` | body `{ championId }` |
| POST | `/api/champ-select/lock` | body `{ championId }` |
| GET | `/api/spells` | `[{ id, name, iconUrl }]`（只回傳目前選角模式可用者；不在選角時為一般模式） |
| POST | `/api/champ-select/spells` | body `{ spell1Id, spell2Id }` |
| POST | `/api/champ-select/bench-swap` | body `{ championId }` |
| POST | `/api/champ-select/reroll` | |
| GET | `/api/perks/pages` | `[{ id, name, isCurrent, isEditable }]` |
| POST | `/api/perks/current` | body `{ pageId }` |
| GET / PUT | `/api/settings` | `{ autoAccept: bool }` |
| GET | `/assets/lcu/*path` | 代理 LCU 圖片，加 `Cache-Control: max-age=86400` |
| GET | `/*` | 前端靜態檔（SPA fallback 到 index.html） |

### 7.9 給手機的 WebSocket `/ws`
- Server → Client，JSON，每則帶 `type`：
  - `{ "type": "state", "data": State }`：連線時與任何變動時送出完整快照（簡單優先，v1 不做 diff）。
- Client → Server：v1 不需要（操作一律走 REST）。
- 每 20 秒送 ping。

---

## 8. State 模型（前後端共用）

```ts
type State = {
  connection: "idle" | "connecting" | "active";
  phase: string;                       // gameflow phase
  autoAccept: boolean;
  readyCheck: null | {
    state: string;                     // InProgress, ...
    playerResponse: string;            // None | Accepted | Declined
    timer: number;                     // 秒
  };
  champSelect: null | {
    gameMode: string;                  // CLASSIC, ARAM, CHERRY, ...；未知時為空字串
    timerMs: number;
    timerPhase: string;                // BAN_PICK, FINALIZATION, ...
    receivedAt: number;                // server 時間戳，前端自行倒數
    myAction: null | {
      kind: "pick" | "ban";
      championId: number;              // 0 = 未選
      completed: boolean;
      isMyTurn: boolean;
    };
    mySelection: { championId: number; spell1Id: number; spell2Id: number; position: string };
    myTeam: Array<{ cellId: number; championId: number; pickIntent: number; position: string; isMe: boolean }>;
    theirTeam: Array<{ championId: number }>;
    bans: { myTeam: number[]; theirTeam: number[] };
    bench: null | number[];            // 替補席英雄；模式沒有替補席時為 null
    rerollsRemaining: null | number;   // 不可重骰時為 null
  };
};
```
Rust 端在 `server` crate 定義對應 struct，`#[serde(rename_all = "camelCase")]`。

---

## 9. 前端（`web/`）

### 9.1 畫面
依 `state.connection` 與 `state.phase` 自動切換，不需路由：

1. **等待中**（idle / connecting）：「等待 LoL 客戶端啟動」。
2. **大廳 / 排隊中**：顯示目前階段、`自動接受` 開關。
3. **Ready Check**：全螢幕兩顆大按鈕「接受」（綠）/「拒絕」（灰），倒數條；已接受後顯示「已接受，等待其他玩家」。
4. **選角**：
   - 頂部：倒數秒數（依 `timerMs` 與 `receivedAt` 本地遞減）、目前是 Ban 還是 Pick、是否輪到我。
   - 我方隊伍列：5 格頭像 + 位置；對方 5 格頭像（只有英雄）。
   - 英雄選擇：搜尋框（比對 name 與 alias，支援中文名稱）+ 頭像網格；不可選者灰階。點擊 = hover，再按底部「Lock in」確認。
   - 召喚師技能：兩格（D / F），點擊開啟技能選擇器。
   - 符文頁：下拉選單。
5. **遊戲中**：「遊戲進行中」。

### 9.2 要求
- Mobile-first，觸控目標至少 48px，支援深色模式（預設深色）。
- WebSocket 斷線自動重連（指數退避，上限 5 秒），斷線時頂部顯示提示條。
- 所有操作按鈕在請求中顯示 loading 並防止重複點擊；失敗顯示 toast。
- 圖片走 `/assets/lcu/...`。
- 不需 service worker，不需 PWA manifest（v1）。

---

## 10. 自動啟動（`install`）

### macOS
- 寫入 `~/Library/LaunchAgents/dev.lol-remote.plist`：
  - `ProgramArguments`: [執行檔絕對路徑, "run"]
  - `RunAtLoad: true`、`KeepAlive: true`
  - `StandardOutPath` / `StandardErrorPath` 指向 log 目錄
- 執行 `launchctl bootstrap gui/$(id -u) <plist>`；`uninstall` 用 `bootout` 並刪檔。
- Daemon 常駐、在 Idle 狀態等待 lockfile，達成「打開 LoL 就啟動」的效果。

### Windows
- 在 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 新增值 `lol-remote` = `"<exe>" run`。
- 以 `windows_subsystem = "windows"` 編譯一個無視窗版本，或文件說明。

---

## 11. Milestones 與驗收標準

### M1 — LCU 基礎
- [ ] 解析 lockfile（含單元測試：正常、缺欄位、空檔）。
- [ ] REST client 能 GET `/lol-gameflow/v1/gameflow-phase`。
- [ ] WS 訂閱 gameflow-phase，終端機印出階段變化。
- 驗收：開 LoL → 進大廳 → 排隊，終端機依序印出 `Lobby → Matchmaking`。

### M2 — Ready check
- [ ] Supervisor 狀態機（Idle/Connecting/Active）。
- [ ] Ready check 偵測與去重。
- [ ] `auto_accept`。
- 驗收：排到隊時終端機印出 ReadyCheck，`auto_accept` 開啟時自動接受；關閉並重開 LoL 後仍能運作。

### M3 — 內網網頁（接受）
- [ ] axum server、token 驗證、rust-embed、`/api/state`、`/ws`。
- [ ] 前端「等待中 / 大廳 / Ready Check」三個畫面。
- [ ] mDNS、`pair` QR code。
- 驗收：手機掃 QR 進入網頁，ready check 時畫面自動切換，按接受成功。

### M4 — 選角
- [ ] champ select 解析、hover / lock、ban。
- [ ] 召喚師技能、符文頁切換。
- [ ] 圖片 proxy。
- [ ] 前端選角畫面。
- 驗收：在一般對戰（盲選與選秀）與積分選秀中，能從手機完成 ban、pick、lock、換技能、換符文頁。

### M5 — 散布
- [ ] `install` / `uninstall`（macOS 必做，Windows 次之）。
- [ ] `caffeinate`。
- [ ] GitHub Actions：build macOS（arm64 + x86_64 universal）與 Windows 執行檔；前端先 build 再 cargo build。
- [ ] README：安裝、配對、已知限制、免責聲明。

---

## 12. 開發與測試

- **mock-lcu crate**：一個本機 HTTPS + WSS server（自簽憑證），模擬：
  - 寫出假 lockfile
  - gameflow phase 依指令切換（例如 stdin 輸入 `readycheck`、`champselect`）；`mode aram` / `mode classic` 切換下一次選角的模式
  - 回應本 spec 用到的所有 endpoint，champ select session 使用固定 fixture JSON
  - `lol-remote` 可用 `--lockfile <path>` 指向 mock，讓整個流程不開遊戲也能測。
- Fixture：在 `crates/lcu/tests/fixtures/` 放真實擷取的 session / ready-check JSON（擷取時移除玩家名稱與 puuid）。
- 單元測試：lockfile 解析、`current_action` 判斷（pick / ban / 非我回合 / 已完成）、State 轉換。
- `cargo clippy -- -D warnings`、`cargo fmt --check` 必須通過。

---

## 13. 已知限制與注意事項

- LCU 為非官方、未承諾穩定的 API，客戶端改版可能導致功能失效。
- Riot 開發者政策禁止「替玩家做決策」的應用；本工具只轉發使用者在手機上的操作，「自動接受」為唯一自動化項目。公開發布前應查閱 Riot 最新的第三方應用政策。
- 選角期間不得顯示其他玩家的名稱或戰績。
- Ready check 僅約 10 秒，手機網頁需保持開啟；建議使用者在離開座位時開啟「自動接受」。
- 手機必須與電腦在同一區網。
- Token 在區網中以明文 HTTP 傳輸，僅適用於信任的家用網路。
