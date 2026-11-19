const COOKIE = "lolr_token";

/** Moves `?t=<token>` from the URL into a cookie (spec §7.8). */
export function adoptTokenFromUrl() {
  const url = new URL(location.href);
  const token = url.searchParams.get("t");
  if (!token) return;
  document.cookie = `${COOKIE}=${encodeURIComponent(token)}; path=/; max-age=31536000; SameSite=Lax`;
  url.searchParams.delete("t");
  history.replaceState(null, "", url.pathname + url.search + url.hash);
}

export class ApiError extends Error {
  constructor(message: string, readonly status: number) {
    super(message);
  }
}

export async function api<T = unknown>(method: string, path: string, body?: unknown): Promise<T> {
  let res: Response;
  try {
    res = await fetch(`/api${path}`, {
      method,
      credentials: "same-origin",
      headers: body === undefined ? undefined : { "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    throw new ApiError("無法連線到電腦", 0);
  }
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new ApiError(data.error ?? `HTTP ${res.status}`, res.status);
  return data as T;
}

export const championIcon = (id: number) =>
  `/assets/lcu/lol-game-data/assets/v1/champion-icons/${id}.png`;
