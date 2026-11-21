import { api } from "./api";
import { ChampSelect } from "./components/ChampSelect";
import { ReadyCheck } from "./components/ReadyCheck";
import { Toasts } from "./components/Toasts";
import { Toggle } from "./components/Toggle";
import { useLiveState, type Live } from "./hooks";
import { toast } from "./toast";
import type { State } from "./types";

const PHASES: Record<string, string> = {
  None: "主畫面",
  Lobby: "大廳",
  Matchmaking: "排隊中…",
  CheckedIntoTournament: "已報名競賽",
  ReadyCheck: "找到對戰",
  ChampSelect: "選角中",
  GameStart: "遊戲載入中",
  FailedToLaunch: "遊戲啟動失敗",
  InProgress: "遊戲進行中",
  Reconnect: "等待重新連線",
  WaitingForStats: "等待結算",
  PreEndOfGame: "遊戲結束",
  EndOfGame: "遊戲結束",
  TerminatedInError: "遊戲異常結束",
};

const IN_GAME = new Set(["GameStart", "InProgress", "Reconnect"]);

export function App() {
  const live = useLiveState();
  return (
    <>
      {!live.online && live.state && !live.unauthorized && (
        <div class="banner" role="alert">
          與電腦的連線中斷，重新連線中…
        </div>
      )}
      <main>
        <Screen live={live} />
      </main>
      <Toasts />
    </>
  );
}

function Screen({ live }: { live: Live }) {
  if (live.unauthorized) {
    return (
      <section class="screen center">
        <h1 class="headline">需要重新配對</h1>
        <p class="muted">
          在電腦上執行 <code>lol-remote pair</code>，再用手機掃描 QR code。
        </p>
      </section>
    );
  }
  const { state } = live;
  if (!state) {
    return (
      <section class="screen center">
        <span class="spinner spinner-lg" />
        <p class="muted">連線到電腦中…</p>
      </section>
    );
  }
  if (state.connection !== "active") {
    return (
      <section class="screen center">
        <div class="pulse" aria-hidden="true" />
        <h1 class="headline">等待 LoL 客戶端啟動</h1>
        <p class="muted">
          {state.connection === "connecting" ? "偵測到客戶端，連線中…" : "打開 League of Legends 後會自動連線。"}
        </p>
        <AutoAccept state={state} />
      </section>
    );
  }
  if (state.phase === "ReadyCheck") {
    return <ReadyCheck rc={state.readyCheck} receivedLocal={live.receivedLocal} />;
  }
  if (state.phase === "ChampSelect" && state.champSelect) {
    return (
      <ChampSelect
        cs={state.champSelect}
        serverTime={state.serverTime}
        receivedLocal={live.receivedLocal}
      />
    );
  }
  if (IN_GAME.has(state.phase)) {
    return (
      <section class="screen center">
        <h1 class="headline">遊戲進行中</h1>
        <p class="muted">祝你順利！遊戲結束後這裡會自動更新。</p>
      </section>
    );
  }
  return (
    <section class="screen center">
      <p class="phase-chip">{PHASES[state.phase] ?? state.phase}</p>
      <h1 class="headline">{state.phase === "Matchmaking" ? "排隊中" : "已連線"}</h1>
      <p class="muted">找到對戰時這裡會自動切換。</p>
      <AutoAccept state={state} />
    </section>
  );
}

function AutoAccept({ state }: { state: State }) {
  const set = (autoAccept: boolean) =>
    api("PUT", "/settings", { autoAccept }).catch(() => toast("無法更新設定"));
  return (
    <div class="card">
      <Toggle
        label="自動接受對戰"
        hint="離開座位時建議開啟"
        checked={state.autoAccept}
        onChange={set}
      />
    </div>
  );
}

