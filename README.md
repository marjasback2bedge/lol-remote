# lol-remote

用手機在區網內遠端操作 League of Legends 客戶端：排到隊時手機跳通知，點開內網網頁即可接受 / 拒絕、Ban、選角、選召喚師技能與切換符文頁。

完整規格見 [`docs/SPEC.md`](docs/SPEC.md)。

## 運作方式

```
LoL 客戶端 (LCU, https://127.0.0.1:<port>)
      ▲  REST + WAMP/WSS
lol-remote（單一執行檔，常駐）
      ├─ HTTP + WebSocket :3000 ──► 手機瀏覽器（同一區網）
      └─ ntfy 推播 ──► ntfy.sh ──► 手機 ntfy App
```

lol-remote 會監看 LoL 的 lockfile：客戶端一開啟就自動連上，關閉後回到閒置狀態等待下次開啟。

## 安裝

需要：Rust（stable）、Node.js 18+。

```sh
cd web && npm ci && npm run build && cd ..   # 先 build 前端（會被內嵌進執行檔）
cargo build --release -p lol-remote
./target/release/lol-remote run
```

首次執行會建立設定檔並在終端機印出配對 QR code。

### 開機自動啟動

```sh
./target/release/lol-remote install     # macOS: ~/Library/LaunchAgents/dev.lol-remote.plist
./target/release/lol-remote uninstall
```

- macOS：以 LaunchAgent 常駐（`RunAtLoad` + `KeepAlive`），log 位於 `~/Library/Application Support/dev.lol-remote.lol-remote/logs/`。客戶端執行期間會以 `caffeinate` 防止電腦睡眠。
- Windows：寫入 `HKCU\...\CurrentVersion\Run`。目前執行時會顯示主控台視窗。

`install` 記錄的是執行檔目前的絕對路徑，搬移執行檔後請重新執行 `install`。

## 配對

```sh
lol-remote pair
```

1. **網頁**：用手機掃第一個 QR code，開啟 `http://<區網 IP>:3000/?t=<token>`。token 會存進 cookie，之後直接開 `http://<電腦名稱>.local:3000` 即可。
2. **推播**：安裝 [ntfy](https://ntfy.sh) App，訂閱第二個 QR code 的 topic。

網頁上的「通知設定」頁（`/pair`）也會顯示 topic。

## 設定檔

`lol-remote config path` 會印出位置（macOS 為 `~/Library/Application Support/dev.lol-remote.lol-remote/config.toml`）。

| 欄位 | 預設 | 說明 |
|---|---|---|
| `port` | `3000` | 網頁埠號 |
| `lockfile_path` | `""` | 空字串 = 平台預設路徑 |
| `auth_token` | 隨機 32 字元 | 手機存取權杖 |
| `ntfy_server` | `https://ntfy.sh` | 可換成自架 ntfy |
| `ntfy_topic` | `lolr-<隨機>` | 等同密碼，請勿公開 |
| `auto_accept` | `false` | 可在網頁切換 |
| `hostname` | `""` | 空字串 = 系統名稱 |
| `notify_cancel` | `true` | 配對取消時推播（低優先度） |
| `notify_use_ip` | `false` | 通知連結改用區網 IP（`.local` 無法解析時開啟） |

## 開發

```
crates/lcu        與 LCU 溝通：lockfile、REST、WAMP 事件、選角邏輯
crates/server     lol-remote 執行檔：狀態機、API、WebSocket、ntfy、mDNS
crates/mock-lcu   假的 LCU（自簽 HTTPS + WSS），不用開遊戲也能測整個流程
web/              Vite + Preact + TypeScript 前端
```

### 用 mock-lcu 測試

```sh
# 終端機 1：假客戶端，會寫出 ./mock-lockfile
cargo run -p mock-lcu
#   指令：lobby | queue（3 秒後 ready check）| rc | cs（選角）| game | none | others-decline | quit

# 終端機 2
cargo run -p lol-remote -- run --lockfile mock-lockfile

# 前端熱更新（選用）：http://localhost:5173/?t=<token>
cd web && LOLR_BACKEND=http://localhost:3000 npm run dev
```

mock 的選角是完整的 10 人選秀：其他 9 名玩家會在輪到時自動 Ban / Pick。

### 檢查

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd web && npm run typecheck
```

CI（`.github/workflows/build.yml`）會跑上述檢查，並產出 macOS universal（arm64 + x86_64）與 Windows 執行檔。

## 安全性

- 所有 `/api/*`、`/ws`、`/assets/lcu/*` 都需要 token（`Authorization: Bearer` 或 `lolr_token` cookie），以 constant-time 比對。
- LCU 的密碼與埠號不會傳給手機；手機只能呼叫上表列出的操作，沒有任意 LCU proxy；圖片 proxy 只允許 `/lol-game-data/assets/`。
- 推播內容不含召喚師名稱或任何個資。
- 區網內為明文 HTTP，**只適用於信任的家用網路**。

## 已知限制

- LCU 是非官方、未承諾穩定的 API，客戶端改版可能讓功能失效。
- 與 LCU 的 TLS 採用規格中的方案 B：只對 `127.0.0.1` 略過憑證驗證（尚未內建 Riot 根憑證）。
- Ready check 只有約 10 秒，請把通知延遲與解鎖手機的時間算進去；離開座位時建議開啟「自動接受」。
- iOS 版 ntfy 的通知按鈕（直接「接受」）需實測；不支援時請點通知開啟網頁再按接受。
- 手機必須與電腦在同一個區網。
- 尚未在真實客戶端上驗收（M1–M4 的驗收步驟需要實際開遊戲）。

## 免責聲明

本專案與 Riot Games 無關，未經 Riot Games 認可。League of Legends 為 Riot Games, Inc. 的商標。lol-remote 只轉發使用者在手機上的操作，「自動接受」是唯一的自動化功能；選角時不顯示其他玩家的名稱或戰績。公開散布前請查閱 Riot 最新的第三方應用政策。
