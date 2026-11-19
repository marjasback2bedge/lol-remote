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
  timerMs: number;
  timerPhase: string;
  receivedAt: number;
  myAction: MyAction | null;
  mySelection: { championId: number; spell1Id: number; spell2Id: number; position: string };
  myTeam: TeamSlot[];
  theirTeam: { championId: number }[];
  bans: { myTeam: number[]; theirTeam: number[] };
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

export type PairInfo = {
  ntfyServer: string;
  ntfyTopic: string;
  subscribeUrl: string;
  mdnsUrl: string;
  ipUrl: string;
};
