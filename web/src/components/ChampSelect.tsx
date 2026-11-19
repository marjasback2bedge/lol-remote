import { useEffect, useMemo, useState } from "preact/hooks";
import { api, ApiError, championIcon } from "../api";
import { useTick } from "../hooks";
import { toast } from "../toast";
import type { ChampSelect as CS, Champion, PerkPage, Spell } from "../types";
import { ActionButton } from "./ActionButton";

const POSITIONS: Record<string, string> = {
  top: "上路",
  jungle: "打野",
  middle: "中路",
  bottom: "下路",
  utility: "輔助",
};

const TIMER_PHASES: Record<string, string> = {
  PLANNING: "宣告階段",
  BAN_PICK: "禁選階段",
  FINALIZATION: "最終準備",
  GAME_STARTING: "遊戲即將開始",
};

function fail(e: unknown) {
  toast(e instanceof ApiError ? e.message : "操作失敗");
}

function Icon({ id, size = "md", dim }: { id: number; size?: "sm" | "md"; dim?: boolean }) {
  if (id <= 0) return <span class={`icon icon-${size} icon-empty`} />;
  return (
    <img
      class={`icon icon-${size}${dim ? " icon-dim" : ""}`}
      src={championIcon(id)}
      alt=""
      loading="lazy"
    />
  );
}

type Props = { cs: CS; serverTime: number; receivedLocal: number };

export function ChampSelect({ cs, serverTime, receivedLocal }: Props) {
  useTick(250);
  const remaining = Math.max(
    0,
    cs.timerMs - (serverTime - cs.receivedAt) - (Date.now() - receivedLocal),
  );
  const action = cs.myAction;
  const isBan = action?.kind === "ban";
  const myTurn = !!action?.isMyTurn;

  const [champions, setChampions] = useState<Champion[]>([]);
  // Pickable/bannable lists change as others lock in; refresh on turn changes.
  useEffect(() => {
    api<Champion[]>("GET", "/champions").then(setChampions).catch(fail);
  }, [action?.kind, myTurn, action?.completed]);

  const names = useMemo(() => new Map(champions.map((c) => [c.id, c.name])), [champions]);
  const lockedPick = cs.myAction?.kind === "pick" && cs.myAction.completed;
  const showGrid = !!action && !action.completed;

  return (
    <section class="screen champ-select">
      <header class="cs-header">
        <div>
          <div class="cs-phase">{TIMER_PHASES[cs.timerPhase] ?? cs.timerPhase}</div>
          <div class={`cs-turn${myTurn ? " cs-turn-mine" : ""}`}>
            {myTurn
              ? isBan
                ? "輪到你禁用"
                : "輪到你選角"
              : lockedPick
                ? `已鎖定 ${names.get(cs.myAction!.championId) ?? ""}`
                : "等待其他玩家"}
          </div>
        </div>
        <div class={`cs-timer${remaining < 10_000 ? " cs-timer-low" : ""}`}>
          {Math.ceil(remaining / 1000)}
        </div>
      </header>

      <Teams cs={cs} />

      {showGrid && (
        <ChampionGrid
          champions={champions}
          isBan={isBan}
          myTurn={myTurn}
          current={action!.championId}
        />
      )}

      <Loadout cs={cs} />
    </section>
  );
}

function Teams({ cs }: { cs: CS }) {
  const bans = [...cs.bans.myTeam, ...cs.bans.theirTeam].filter((b) => b > 0);
  return (
    <div class="teams">
      <div class="team">
        {cs.myTeam.map((m) => (
          <div key={m.cellId} class={`slot${m.isMe ? " slot-me" : ""}`}>
            <Icon id={m.championId || m.pickIntent} dim={!m.championId} />
            <span class="slot-pos">{POSITIONS[m.position] ?? (m.isMe ? "你" : "")}</span>
          </div>
        ))}
      </div>
      {cs.theirTeam.length > 0 && (
        <div class="team team-enemy">
          {cs.theirTeam.map((m, i) => (
            <div key={i} class="slot">
              <Icon id={m.championId} />
            </div>
          ))}
        </div>
      )}
      {bans.length > 0 && (
        <div class="bans" aria-label="禁用">
          {bans.map((b, i) => (
            <Icon key={i} id={b} size="sm" />
          ))}
        </div>
      )}
    </div>
  );
}

type GridProps = { champions: Champion[]; isBan: boolean; myTurn: boolean; current: number };

function ChampionGrid({ champions, isBan, myTurn, current }: GridProps) {
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(current);
  useEffect(() => setSelected(current), [current]);

  const q = query.trim().toLowerCase();
  const shown = champions.filter(
    (c) => !q || c.name.toLowerCase().includes(q) || c.alias.toLowerCase().includes(q),
  );
  const available = (c: Champion) => (isBan ? c.bannable : c.pickable);

  const hover = async (c: Champion) => {
    if (!available(c)) return;
    setSelected(c.id);
    try {
      await api("POST", "/champ-select/hover", { championId: c.id });
    } catch (e) {
      fail(e);
    }
  };

  const selectedChamp = champions.find((c) => c.id === selected);

  return (
    <div class="picker">
      <input
        class="search"
        type="search"
        placeholder="搜尋英雄（中文或英文）"
        value={query}
        onInput={(e) => setQuery((e.target as HTMLInputElement).value)}
      />
      <div class="grid">
        {shown.map((c) => (
          <button
            key={c.id}
            class={`champ${c.id === selected ? " champ-selected" : ""}${available(c) ? "" : " champ-disabled"}`}
            disabled={!available(c)}
            onClick={() => hover(c)}
          >
            <img src={c.iconUrl} alt="" loading="lazy" />
            <span>{c.name}</span>
          </button>
        ))}
        {shown.length === 0 && <p class="muted">找不到英雄</p>}
      </div>
      <div class="lock-bar">
        <ActionButton
          class={isBan ? "btn-ban" : "btn-accept"}
          disabled={!myTurn || !selectedChamp || !available(selectedChamp)}
          onClick={() => api("POST", "/champ-select/lock", { championId: selected })}
        >
          {!myTurn
            ? "尚未輪到你"
            : selectedChamp
              ? `${isBan ? "禁用" : "鎖定"} ${selectedChamp.name}`
              : isBan
                ? "選擇要禁用的英雄"
                : "選擇英雄"}
        </ActionButton>
      </div>
    </div>
  );
}

function Loadout({ cs }: { cs: CS }) {
  const [spells, setSpells] = useState<Spell[]>([]);
  const [pages, setPages] = useState<PerkPage[]>([]);
  const [editing, setEditing] = useState<0 | 1 | 2>(0);

  useEffect(() => {
    api<Spell[]>("GET", "/spells").then(setSpells).catch(fail);
    api<PerkPage[]>("GET", "/perks/pages").then(setPages).catch(fail);
  }, []);

  const { spell1Id, spell2Id } = cs.mySelection;
  const spell = (id: number) => spells.find((s) => s.id === id);

  const choose = async (id: number) => {
    const slot = editing;
    setEditing(0);
    let [a, b] = [spell1Id, spell2Id];
    if (slot === 1) [a, b] = [id, id === b ? a : b];
    else [a, b] = [id === a ? b : a, id];
    try {
      await api("POST", "/champ-select/spells", { spell1Id: a, spell2Id: b });
    } catch (e) {
      fail(e);
    }
  };

  const setPage = async (id: number) => {
    const previous = pages;
    setPages(pages.map((p) => ({ ...p, isCurrent: p.id === id })));
    try {
      await api("POST", "/perks/current", { pageId: id });
    } catch (e) {
      setPages(previous);
      fail(e);
    }
  };

  return (
    <div class="loadout">
      <div class="spells">
        {([1, 2] as const).map((slot) => {
          const s = spell(slot === 1 ? spell1Id : spell2Id);
          return (
            <button key={slot} class="spell-slot" onClick={() => setEditing(slot)}>
              {s ? <img src={s.iconUrl} alt="" /> : <span class="icon icon-md icon-empty" />}
              <span>
                <kbd>{slot === 1 ? "D" : "F"}</kbd> {s?.name ?? "未設定"}
              </span>
            </button>
          );
        })}
      </div>
      <label class="runes">
        <span>符文頁</span>
        <select
          value={pages.find((p) => p.isCurrent)?.id ?? ""}
          onChange={(e) => setPage(Number((e.target as HTMLSelectElement).value))}
        >
          {pages.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </label>

      {editing !== 0 && (
        <div class="sheet-backdrop" onClick={() => setEditing(0)}>
          <div class="sheet" role="dialog" aria-label="選擇召喚師技能" onClick={(e) => e.stopPropagation()}>
            <h2>選擇 {editing === 1 ? "D" : "F"} 技能</h2>
            <div class="spell-grid">
              {spells.map((s) => (
                <button key={s.id} class="spell-option" onClick={() => choose(s.id)}>
                  <img src={s.iconUrl} alt="" />
                  <span>{s.name}</span>
                </button>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
