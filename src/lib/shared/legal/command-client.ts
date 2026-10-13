import { workFetch } from "@/lib/work_projection/work-fetch";

type Attempt = { path: string; body: string; key: string };

/** Keep an uncertain command's exact payload and key across a refresh. */
export function createLegalCommandClient(storageKey: string) {
  let attempt: Attempt | null = null;
  let pending: Promise<unknown> | null = null;
  function forget() {
    attempt = null;
    try { sessionStorage.removeItem(storageKey); } catch {}
  }
  function savedAttempt() {
    try {
      const saved = sessionStorage.getItem(storageKey);
      if (saved) {
        const value = JSON.parse(saved);
        if (typeof value.path === "string" && typeof value.body === "string" && typeof value.key === "string") attempt = value;
      }
    } catch {}
    return attempt;
  }
  const send = async function (path: string, input: unknown): Promise<unknown> {
    const body = JSON.stringify(input);
    if (pending) throw new Error("Wait for the current Legal action to finish.");
    savedAttempt();
    if (attempt && (attempt.path !== path || attempt.body !== body)) {
      throw new Error("A previous action has an uncertain result. Retry its original details before sending a different action.");
    }
    attempt ??= { path, body, key: crypto.randomUUID() };
    const current = attempt;
    try { sessionStorage.setItem(storageKey, JSON.stringify(current)); } catch {}
    pending = (async () => {
      const response = await workFetch(path, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": current.key },
        body: current.body,
      });
      const rejected = response.status >= 400 && response.status < 500 && ![408, 429].includes(response.status);
      if (rejected) forget();
      const result = await response.json().catch(() => {
        throw new Error(rejected ? "The request was rejected. Reload and review before submitting." : "Unable to read the result. Reconnect and recover this action.");
      });
      if (!response.ok) {
        throw new Error(result.error?.message ?? "Unable to save. Reconnect and retry this action.");
      }
      forget();
      return result.data;
    })();
    try { return await pending; } finally { pending = null; }
  };
  return Object.assign(send, {
    hasPending: () => Boolean(savedAttempt()),
    recover: async () => {
      const current = savedAttempt();
      if (!current) return null;
      const input = JSON.parse(current.body) as Record<string, unknown>;
      return { input, data: await send(current.path, input) };
    },
  });
}
