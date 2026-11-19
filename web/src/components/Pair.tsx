import { useEffect, useState } from "preact/hooks";
import { api, ApiError } from "../api";
import type { PairInfo } from "../types";

export function Pair() {
  const [info, setInfo] = useState<PairInfo | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    api<PairInfo>("GET", "/pair")
      .then(setInfo)
      .catch((e) => setError(e instanceof ApiError ? e.message : "載入失敗"));
  }, []);

  return (
    <section class="screen pair">
      <h1 class="headline">通知設定</h1>
      {error && <p class="status">{error}</p>}
      {info && (
        <ol class="steps">
          <li>
            安裝 <strong>ntfy</strong> App（iOS / Android）。
          </li>
          <li>
            新增訂閱，伺服器填 <code>{info.ntfyServer}</code>，Topic 填：
            <code class="copy">{info.ntfyTopic}</code>
            或直接開啟 <a href={info.subscribeUrl}>{info.subscribeUrl}</a>
          </li>
          <li>
            Topic 等同密碼，請勿公開分享。推播內容不含任何個人資料。
          </li>
          <li>
            此網頁網址：<code>{info.mdnsUrl}</code>（或 <code>{info.ipUrl}</code>）
          </li>
        </ol>
      )}
      <a class="btn btn-secondary" href="/">
        返回
      </a>
    </section>
  );
}
