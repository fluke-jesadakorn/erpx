import { workFetch } from "@/lib/work_projection/work-fetch";

/** Keep the command identity after an uncertain response, scoped to the rendered work role. */
export function createFinanceCommandFetch(send: typeof workFetch = workFetch) {
  const attempts = new Map<string, string>();
  return async function financeRequest(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
    const method = (init?.method ?? (input instanceof Request ? input.method : "GET")).toUpperCase();
    if (!["POST", "PUT", "PATCH", "DELETE"].includes(method) || typeof init?.body !== "string") return send(input, init);
    const context = typeof document === "undefined" ? null : document.querySelector<HTMLElement>("[data-work-revision]");
    const stamp = context ? [context.dataset.workUser, context.dataset.workRevision, context.dataset.workCompany, context.dataset.workAssignment] : [];
    const signature = JSON.stringify([input instanceof Request ? input.url : String(input), method, stamp, init.body]);
    const headers = new Headers(init.headers);
    let key = attempts.get(signature);
    if (!key) {
      key = crypto.randomUUID();
      attempts.set(signature, key);
    }
    headers.set("Idempotency-Key", key);
    const response = await send(input, { ...init, headers });
    if (response.ok) {
      if (response.status === 204) attempts.delete(signature);
      else {
        const acknowledged = await response.clone().json().catch(() => null);
        if (acknowledged?.data !== undefined) attempts.delete(signature);
      }
    }
    return response;
  };
}

export const financeCommandFetch = createFinanceCommandFetch();
