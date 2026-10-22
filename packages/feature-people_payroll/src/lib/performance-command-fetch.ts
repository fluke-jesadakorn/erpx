import {workFetch, type WorkCommandCompletion} from "@erpx/feature-work_projection/lib/work-fetch";
import {announceWorkChange} from "@erpx/feature-work_projection/lib/work-refresh";

export type PerformancePurchase = {newWork: boolean; settleExisting: boolean; history: boolean; revision: string};
export function performanceCommandContext() {
  const node = typeof document === "undefined" ? null : document.querySelector<HTMLElement>("[data-work-revision]");
  return node ? [node.dataset.workUser, node.dataset.workRevision, node.dataset.workCompany, node.dataset.workAssignment].join(":") : "server";
}
export function capturePerformanceForm(form?: HTMLFormElement): WorkCommandCompletion | undefined {
  const selected = form ?? (typeof document === "undefined" ? null : document.activeElement?.closest<HTMLFormElement>("form[data-performance-draft]"));
  return selected ? {form: selected, formEditVersion: selected.getAttribute("data-erpx-edit-version")} : undefined;
}
export function performanceFormUnchanged(completion: {form: Pick<HTMLFormElement,"isConnected"|"getAttribute">; formEditVersion: string|null} | undefined) {
  return !completion || completion.form.isConnected && completion.form.getAttribute("data-erpx-edit-version") === completion.formEditVersion;
}
export function validPerformanceAcknowledgement(value: unknown): value is Record<string, unknown> & {id: string} {
  return !!value && typeof value === "object" && !Array.isArray(value) && "id" in value && typeof value.id === "string" && value.id.length > 0;
}
export function performanceCommandPurpose(path: string, input: unknown): "newWork" | "settleExisting" {
  if(path === "criteria" && input && typeof input === "object" && "id" in input && typeof input.id === "string" && "kind" in input && input.kind === "cycle" && "data" in input && input.data && typeof input.data === "object" && "status" in input.data && input.data.status === "closed") return "settleExisting";
  if (path.endsWith("/transition")) return input && typeof input === "object" && "action" in input && input.action === "reopen" ? "newWork" : "settleExisting";
  return path.endsWith("/evidence") || /^compensation\/[^/]+\/(approve|cancel)$/.test(path) ? "settleExisting" : "newWork";
}
export class PerformanceCommandError extends Error {
  constructor(message: string, readonly uncertain: boolean) {super(message);}
}
/** A successful HTTP status alone does not acknowledge an owner command.
 * Exact context, body and key survive network loss and incomplete responses. */
export function createPerformanceCommandFetch(send: typeof workFetch = workFetch) {
  const attempts = new Map<string, string>();
  const identity = (url: string, input: unknown) => JSON.stringify([performanceCommandContext(), url, JSON.stringify(input)]);
  const request = async (url: string, input: unknown, completion = capturePerformanceForm()) => {
    const identityKey = identity(url, input);
    let key = attempts.get(identityKey);
    if (!key) {key = crypto.randomUUID(); attempts.set(identityKey, key);}
    let response: Response;
    try {response = await send(url, {method: "POST", headers: {"Content-Type": "application/json", "Idempotency-Key": key}, body: JSON.stringify(input)}, completion, "caller");}
    catch {throw new PerformanceCommandError("The result is unknown. Retry the submitted action before sending another change.", true);}
    const payload: unknown = await response.json().catch(() => null);
    const data = payload && typeof payload === "object" && "data" in payload ? payload.data : null;
    const error = payload && typeof payload === "object" && "error" in payload ? payload.error : null;
    if (!response.ok) {
      const message = error && typeof error === "object" && "message" in error && typeof error.message === "string" ? error.message : "Could not save Evaluation changes.";
      const changedContext = error && typeof error === "object" && "code" in error && error.code === "WORK_CONTEXT_CHANGED";
      throw new PerformanceCommandError(message, !!changedContext);
    }
    if (!validPerformanceAcknowledgement(data)) throw new PerformanceCommandError("The Evaluation acknowledgement is incomplete. Retry the submitted action.", true);
    attempts.delete(identityKey);
    if (typeof window !== "undefined") {
      if (completion) window.dispatchEvent(new CustomEvent("erpx:record-command-saved", {detail: {...completion, performanceAcknowledged: true}}));
      announceWorkChange();
    }
    return data;
  };
  request.hasAttempt = (url: string, input: unknown) => attempts.has(identity(url, input));
  return request;
}
