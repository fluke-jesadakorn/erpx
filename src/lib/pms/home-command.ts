import { workFetch } from "@/lib/work_projection/work-fetch";
import { captureWorkCommandContext } from "@/lib/work_projection/command-context";
import { retainWorkCommandCompletion } from "@/lib/work_projection/command-completion";
import { announceWorkChange } from "@/lib/work_projection/work-refresh";

export type PmsHomeSource = { api: string; property: string; selectedBranch: string | null | undefined };
export type PmsHomeAttempt = { endpoint: string; input: unknown; body: string; key: string; scope: string; completion: ReturnType<typeof retainWorkCommandCompletion>; receiptToken?: symbol };
export type PmsHomeOutcome = { data: { id: string; version?: number }; acknowledge: () => boolean };
function scope(source: PmsHomeSource | null) {
  if (!source) return "disposed";
  const work = typeof document === "undefined" ? null : document.querySelector<HTMLElement>("[data-work-revision]");
  const front = typeof document === "undefined" ? null : document.querySelector<HTMLElement>("[data-front-app]");
  return JSON.stringify([source, work && [work.dataset.workUser, work.dataset.workRevision, work.dataset.workCompany, work.dataset.workAssignment], front && [front.dataset.frontApp, front.dataset.frontLocation]]);
}
export function capturePmsHomeContext(source: () => PmsHomeSource | null) {
  if (!source()) throw Error("The original hotel view is no longer available. Return before retrying.");
  const original = scope(source()), current = captureWorkCommandContext();
  return () => {
    current();
    if (scope(source()) !== original) throw Object.assign(new Error("Your hotel or work context changed. Return to the original context before retrying."), { code: "WORK_CONTEXT_CHANGED" });
  };
}

/** Keep the exact serialized request and key through receipt decoding and the
 * caller's required reads, until guarded explicit acknowledgement consumes it. */
export async function sendPmsHomeCommand(attempts: Map<string, PmsHomeAttempt>, source: () => PmsHomeSource | null, endpoint: string, input: unknown): Promise<PmsHomeOutcome> {
  if (!source()) throw Error("The original hotel view is no longer available. Return before retrying.");
  const body = JSON.stringify(input), fingerprint = JSON.stringify([endpoint, input]);
  let attempt = attempts.get(fingerprint);
  const first = !attempt;
  if (!attempt) {
    if ([...attempts.values()].some(value => value.endpoint === endpoint)) throw Error("Retry the submitted action first. Your newer draft is retained.");
    const form = typeof document === "undefined" ? null : document.activeElement?.closest<HTMLFormElement>("form");
    attempt = { endpoint, input: JSON.parse(body), body, key: crypto.randomUUID(), scope: scope(source()), completion: retainWorkCommandCompletion(form ? { form, formEditVersion: form.getAttribute("data-erpx-edit-version") } : undefined) };
    attempts.set(fingerprint, attempt);
  }
  if (attempt.scope !== scope(source())) throw Error("Return to the original hotel and work context before retrying.");
  const token = Symbol("pms-home-request");
  attempt.receiptToken = token;
  const assertCurrent = capturePmsHomeContext(source);
  const response = await workFetch(endpoint, { method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.key }, body: attempt.body }, attempt.completion(), "caller");
  const result: unknown = await response.json();
  if (!response.ok) {
    const error = result && typeof result === "object" && "error" in result ? result.error : null;
    // This exact native handover rejection precedes savePostDraft. A prior
    // uncertain/retried request must keep its original key instead.
    if (first && endpoint === `${source()?.api}/handover` && response.status === 409 && error && typeof error === "object" && "code" in error && error.code === "HANDOVER_SOURCE_CHANGED") {
      assertCurrent();
      if (attempts.get(fingerprint) === attempt && attempt.receiptToken === token) attempts.delete(fingerprint);
    }
    throw Object.assign(new Error(error && typeof error === "object" && "message" in error ? String(error.message) : "Please try again."), { status: response.status });
  }
  const data = result && typeof result === "object" && "data" in result ? result.data : null;
  const post = /\/posts\/([^/]+)$/.exec(endpoint);
  const version = data && typeof data === "object" && "version" in data ? data.version : undefined;
  const action = input && typeof input === "object" && "action" in input ? input.action : undefined;
  const expected = input && typeof input === "object" && "expectedVersion" in input ? input.expectedVersion : undefined;
  if (!data || typeof data !== "object" || !("id" in data) || typeof data.id !== "string" || !data.id
    || (version !== undefined && (typeof version !== "number" || !Number.isSafeInteger(version) || version <= 0))
    || (post ? data.id !== post[1] || action === "edit" && (typeof expected !== "number" || !Number.isSafeInteger(expected) || expected <= 0 || expected >= Number.MAX_SAFE_INTEGER || version !== expected + 1)
      || action === "retract" && version === undefined : version !== 1))
    throw Error("The publication acknowledgement is incomplete. Retry the submitted action.");
  assertCurrent();
  const retained = attempt;
  return { data: { id: data.id, ...(typeof version === "number" ? { version } : {}) }, acknowledge() {
    assertCurrent();
    if (attempts.get(fingerprint) !== retained || retained.receiptToken !== token) return false;
    retained.receiptToken = undefined;
    attempts.delete(fingerprint);
    if (typeof window !== "undefined") {
      window.dispatchEvent(new CustomEvent("erpx:record-command-saved", { detail: retained.completion() ?? { form: null, formEditVersion: null } }));
      announceWorkChange();
    }
    return true;
  } };
}
