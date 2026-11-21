import { workFetch, type WorkCommandCompletion } from "@erpx/feature-work_projection/lib/work-fetch";
import { announceWorkChange } from "@erpx/feature-work_projection/lib/work-refresh";

export function qualityCommandContext() {
  const node = typeof document === "undefined" ? null : document.querySelector<HTMLElement>("[data-work-revision]");
  return node ? JSON.stringify([node.dataset.workUser, node.dataset.workRevision, node.dataset.workCompany, node.dataset.workAssignment,node.dataset.workAssignmentVersion]) : "server";
}
export function captureQualityForm(): WorkCommandCompletion | undefined {
  const form = typeof document === "undefined" ? null : document.activeElement?.closest<HTMLFormElement>("form[data-quality-draft]");
  return form ? {form, formEditVersion: form.getAttribute("data-erpx-edit-version")} : undefined;
}
export function qualityFormUnchanged(completion: WorkCommandCompletion | undefined) {
  return !completion || completion.form.isConnected && completion.form.getAttribute("data-erpx-edit-version") === completion.formEditVersion;
}
export function validQualityAcknowledgement(value: unknown): value is Record<string, unknown> & {id: string;version:number} {
  return !!value && typeof value === "object" && !Array.isArray(value) && "id" in value && typeof value.id === "string" && value.id.length > 0 && "version" in value && Number.isSafeInteger(value.version) && Number(value.version) > 0;
}
/** Retain exact command identity through network loss or incomplete 2xx
 * acknowledgement. Contexts and changed payloads have separate identities. */
export function createQualityCommandFetch(send: typeof workFetch = workFetch) {
  const attempts = new Map<string, string>();
  const identity = (url: string, body: BodyInit | null | undefined) => JSON.stringify([qualityCommandContext(), url, body ?? null]);
  const request = async (url: string, init: RequestInit, completion = captureQualityForm()) => {
    const id = identity(url, init.body), headers = new Headers(init.headers);
    let key = attempts.get(id);
    if (!key) {key = crypto.randomUUID(); attempts.set(id, key);}
    headers.set("Idempotency-Key", key);
    const response = await send(url, {...init, headers}, completion, "caller");
    if (response.ok) {
      const acknowledged = validQualityAcknowledgement((await response.clone().json().catch(() => null))?.data);
      if (acknowledged) {
        attempts.delete(id);
        if (typeof window !== "undefined") {
          if (completion) window.dispatchEvent(new CustomEvent("erpx:record-command-saved", {detail: {...completion, qualityAcknowledged: true}}));
          announceWorkChange();
        }
      }
    }
    return response;
  };
  request.hasAttempt = (url: string, body: BodyInit | null | undefined) => attempts.has(identity(url, body));
  return request;
}
export const qualityCommandFetch = createQualityCommandFetch();
