import { api } from "../api";
import { useTick } from "../hooks";
import type { ReadyCheck as RC } from "../types";
import { ActionButton } from "./ActionButton";

const TOTAL_SECONDS = 10;

export function ReadyCheck({ rc, receivedLocal }: { rc: RC | null; receivedLocal: number }) {
  useTick(200);
  const elapsed = (rc?.timer ?? 0) + (Date.now() - receivedLocal) / 1000;
  const left = Math.max(0, TOTAL_SECONDS - elapsed);
  const response = rc?.playerResponse ?? "None";

  return (
    <section class="screen ready-check">
      <h1 class="headline">找到對戰！</h1>
      <div class="countdown" aria-label={`剩餘 ${Math.ceil(left)} 秒`}>
        <div class="countdown-bar" style={{ width: `${(left / TOTAL_SECONDS) * 100}%` }} />
      </div>
      <p class="muted big-number">{Math.ceil(left)}</p>
      {response === "Accepted" ? (
        <p class="status ok">已接受，等待其他玩家…</p>
      ) : response === "Declined" ? (
        <p class="status">已拒絕</p>
      ) : (
        <div class="rc-buttons">
          <ActionButton class="btn-accept btn-huge" onClick={() => api("POST", "/ready-check/accept")}>
            接受
          </ActionButton>
          <ActionButton class="btn-secondary" onClick={() => api("POST", "/ready-check/decline")}>
            拒絕
          </ActionButton>
        </div>
      )}
    </section>
  );
}
