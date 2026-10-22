import { workFetch, type WorkCommandCompletion } from "@erpx/feature-work_projection/lib/work-fetch";
import { announceWorkChange } from "@erpx/feature-work_projection/lib/work-refresh";
import { captureWorkCommandContext } from "@erpx/feature-work_projection/lib/command-context";
import { retainWorkCommandCompletion } from "@erpx/feature-work_projection/lib/command-completion";

export function commerceCommandContext() {
  const node = typeof document === "undefined" ? null : document.querySelector<HTMLElement>("[data-work-revision]");
  return node ? [node.dataset.workUser, node.dataset.workRevision, node.dataset.workCompany, node.dataset.workAssignment].join(":") : "server";
}
export function captureCommerceForm(): WorkCommandCompletion | undefined {
  const form = typeof document === "undefined" ? null : document.activeElement?.closest<HTMLFormElement>("form[data-commerce-draft]");
  return form ? {form, formEditVersion: form.getAttribute("data-erpx-edit-version")} : undefined;
}
export function commerceFormUnchanged(completion: WorkCommandCompletion | undefined) {
  return !completion || completion.form.isConnected && completion.form.getAttribute("data-erpx-edit-version") === completion.formEditVersion;
}
export function validCommerceAcknowledgement(value: unknown): value is Record<string, unknown> & {id: string} {
  return !!value && typeof value === "object" && !Array.isArray(value) && "id" in value && typeof value.id === "string" && value.id.length > 0;
}
/** Retain exact command identity through network loss or incomplete 2xx
 * acknowledgement. Contexts and changed payloads have separate identities. */
export function createCommerceCommandFetch(send: typeof workFetch = workFetch) {
  const attempts = new Map<string, {key:string;completion:ReturnType<typeof retainWorkCommandCompletion>}>();
  const identity = (url: string, body: BodyInit | null | undefined) => JSON.stringify([commerceCommandContext(), url, body ?? null]);
  const receipts = new WeakMap<Response, () => boolean>();
  const acknowledge = (response: Response) => receipts.get(response)?.() ?? false;
  const request = async (url: string, init: RequestInit, completion = captureCommerceForm(), acknowledgement: "automatic" | "caller" = "automatic") => {
    const id = identity(url, init.body), headers = new Headers(init.headers);
    let attempt = attempts.get(id);
    if (!attempt) {attempt = {key:crypto.randomUUID(),completion:retainWorkCommandCompletion(completion)}; attempts.set(id, attempt);}
    const originalCompletion=attempt.completion();headers.set("Idempotency-Key", attempt.key);
    const assertCurrent=captureWorkCommandContext(),response = await send(url, {...init, headers}, originalCompletion, "caller");
    if (response.ok) {
      const acknowledged = validCommerceAcknowledgement((await response.clone().json().catch(() => null))?.data);
      if (acknowledged) {
        assertCurrent();if(id!==identity(url,init.body))throw Object.assign(new Error("Return to the original Commerce context before retrying."),{code:"WORK_CONTEXT_CHANGED"});
        const retainedAttempt=attempt;
        receipts.set(response,()=>{
          assertCurrent();if(id!==identity(url,init.body))throw Object.assign(new Error("Return to the original Commerce context before retrying."),{code:"WORK_CONTEXT_CHANGED"});
          if(attempts.get(id)!==retainedAttempt)return false;
          attempts.delete(id);
          if (typeof window !== "undefined") {
            const capturedCompletion=retainedAttempt.completion();
            if (capturedCompletion) window.dispatchEvent(new CustomEvent("erpx:record-command-saved", {detail: {...capturedCompletion, commerceAcknowledged: true}}));
            announceWorkChange();
          }
          receipts.delete(response);
          return true;
        });
        if(acknowledgement==="automatic")acknowledge(response);
      }
    }
    return response;
  };
  request.acknowledge = acknowledge;
  request.hasAttempt = (url: string, body: BodyInit | null | undefined) => attempts.has(identity(url, body));
  request.completion = (url: string, body: BodyInit | null | undefined) => attempts.get(identity(url, body))?.completion();
  return request;
}
export const commerceCommandFetch = createCommerceCommandFetch();
