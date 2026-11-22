// Mirrors the server's State (spec §8).

export type Connection = "idle" | "connecting" | "active";

export type ReadyCheck = {
  state: string;
  playerResponse: string;
  timer: number;
};

export type MyAction = {
  kind: "pick" | "ban";
  championId: number;
  completed: boolean;
  isMyTurn: boolean;
};

export type TeamSlot = {
  cellId: number;
  championId: number;
  pickIntent: number;
  position: string;
  isMe: boolean;
};

export type ChampSelect = {
  /** `CLASSIC`, `ARAM`, ...; empty if unknown. */
  gameMode: string;
  timerMs: number;
  timerPhase: string;
  receivedAt: number;
  myAction: MyAction | null;
  mySelection: { championId: number; spell1Id: number; spell2Id: number; position: string };
  myTeam: TeamSlot[];
  theirTeam: { championId: number }[];
  bans: { myTeam: number[]; theirTeam: number[] };
  /** Null when the mode has no bench. */
  bench: number[] | null;
  /** Null when the mode doesn't allow rerolling. */
  rerollsRemaining: number | null;
};

export type State = {
  connection: Connection;
  phase: string;
  autoAccept: boolean;
  readyCheck: ReadyCheck | null;
  champSelect: ChampSelect | null;
  serverTime: number;
};

export type Champion = {
  id: number;
  name: string;
  alias: string;
  iconUrl: string;
  pickable: boolean;
  bannable: boolean;
};

export type Spell = { id: number; name: string; iconUrl: string };

export type PerkPage = { id: number; name: string; isCurrent: boolean; isEditable: boolean };
