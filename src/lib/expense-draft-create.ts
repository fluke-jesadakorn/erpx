
import { workFetch } from "@/lib/work-fetch";
type DraftAttempt = { body: string; key: string; touched?: string[] };
/** Persist before sending so a lost response or refresh replays the same command. */
export function createExpenseDraftOnce() {
  let attempt: DraftAttempt | undefined;
  let running: Promise<string> | undefined;
  return (base: string, storageKey: string, fields: Record<string, string>, touched: string[] = []) => {
    if (running) return running;
    try {
      const saved = sessionStorage.getItem(`${storageKey}:creation`);
      if (saved) attempt = JSON.parse(saved) as DraftAttempt;
    } catch {}
    attempt ??= { body: JSON.stringify(fields), key: crypto.randomUUID(), touched };
    const command = attempt;
    try { sessionStorage.setItem(`${storageKey}:creation`, JSON.stringify(command)); } catch {}
    running = (async () => {
      const response = await workFetch(`/api/v1${base}/expenses`, {
        method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": command.key }, body: command.body,
      });
      const result = await response.json();
      if (!response.ok) {
        if (response.status >= 400 && response.status < 500 && ![408, 429].includes(response.status)) {
          attempt = undefined;
          try { sessionStorage.removeItem(`${storageKey}:creation`); } catch {}
        }
        throw Error(result.error?.message ?? "Unable to create your draft.");
      }
      const id = String(result.data.id);
      try {
        sessionStorage.setItem(`${storageKey}:created`, id);
        sessionStorage.setItem(`${storageKey.replace(/:new$/, `:${id}`)}:touched`, JSON.stringify(command.touched ?? []));
        sessionStorage.removeItem(`${storageKey}:creation`);
      } catch {}
      return id;
    })().finally(() => { running = undefined; });
    return running;
  };
}
