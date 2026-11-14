import { workFetch, announceWorkChange, captureWorkCommandContext, retainWorkCommandCompletion } from "@erpx/feature-work_projection/public/client";
import { financeCommandSignature } from "../../domain/command-attempt";
import { createFinanceCommandAttempts } from "../../application/command-attempts";

/** Keep the command identity after an uncertain response, scoped to the rendered work role. */
export function createFinanceCommandFetch(send: typeof workFetch = workFetch) {
  const attempts = createFinanceCommandAttempts({
    createKey: () => crypto.randomUUID(),
    retainCompletion: () => {
      const form = typeof document === "undefined" ? null : document.activeElement?.closest<HTMLFormElement>("form");
      return retainWorkCommandCompletion(form ? { form, formEditVersion: form.getAttribute("data-erpx-edit-version") } : undefined);
    },
  });
  function attemptSignature(input:RequestInfo|URL,method:string,body:string) {
    const context = typeof document === "undefined" ? null : document.querySelector<HTMLElement>("[data-work-revision]");
    const stamp = context ? [context.dataset.workUser, context.dataset.workRevision, context.dataset.workCompany, context.dataset.workAssignment,context.dataset.workAssignmentVersion] : [];
    return financeCommandSignature({ target: input instanceof Request ? input.url : String(input), method, scope: stamp, body });
  }
  // Callers that decode the original response defer local completion until
  // their own receipt/context check; a clone alone cannot release the key.
  const receipts = new WeakMap<Response, () => boolean>();
  const acknowledge = (response: Response) => receipts.get(response)?.() ?? false;
  async function financeRequest(input: RequestInfo | URL, init?: RequestInit, acknowledgement: "automatic" | "caller" = "automatic"): Promise<Response> {
    const method = (init?.method ?? (input instanceof Request ? input.method : "GET")).toUpperCase();
    if (!["POST", "PUT", "PATCH", "DELETE"].includes(method) || typeof init?.body !== "string") return send(input, init);
    const signature = attemptSignature(input,method,init.body);
    const headers = new Headers(init.headers);
    const attempt = attempts.retain(signature);
    headers.set("Idempotency-Key", attempt.key);
    const completion=attempt.completion(),assertCurrent=captureWorkCommandContext();
    const response = await send(input, { ...init, headers },completion,"caller");
    if (response.ok) {
      let acknowledged=response.status===204;
      if (!acknowledged) {
        const body = await response.clone().json().catch(() => null);
        acknowledged=body?.data !== undefined;
      }
      if (acknowledged) {
        assertCurrent();
        if(signature!==attemptSignature(input,method,init.body))throw Object.assign(new Error('Return to the original Finance command before retrying.'),{code:'WORK_CONTEXT_CHANGED'});
        const retainedAttempt=attempt;
        receipts.set(response,()=>{
          assertCurrent();
          if(signature!==attemptSignature(input,method,init.body as string))throw Object.assign(new Error('Return to the original Finance command before retrying.'),{code:'WORK_CONTEXT_CHANGED'});
          if(!attempts.acknowledge(signature,retainedAttempt))return false;
          if(typeof window!=="undefined") {
            const originalCompletion=retainedAttempt.completion();
            if(originalCompletion){
              window.dispatchEvent(new CustomEvent("erpx:record-command-saved",{detail:{...originalCompletion,financeAcknowledged:true}}));
              window.dispatchEvent(new CustomEvent("erpx:finance-command-acknowledged",{detail:originalCompletion}));
            }
            announceWorkChange();
          }
          receipts.delete(response);
          return true;
        });
        if(acknowledgement==="automatic")acknowledge(response);
      }
    }
    return response;
  }
  return Object.assign(financeRequest,{
    acknowledge,
    hasAttempt:(input:RequestInfo|URL,body:string,method="POST")=>attempts.has(attemptSignature(input,method,body)),
    completion:(input:RequestInfo|URL,body:string,method="POST")=>attempts.completion(attemptSignature(input,method,body))?.(),
  });
}

export const financeCommandFetch = createFinanceCommandFetch();

/** Decode transport outcomes at the edge; application code never receives Response. */
export async function submitFinanceCommand(input: { readonly target: string; readonly body: string }) {
  const response = await financeCommandFetch(input.target, { method: "POST", headers: { "Content-Type": "application/json" }, body: input.body }, "caller");
  const result = await response.json();
  if (!response.ok) return {
    kind: "rejected" as const,
    definitive: response.status < 500 && result.error?.code !== "WORK_CONTEXT_CHANGED",
    message: result.error?.message ?? "The Finance action could not be saved.",
  };
  return { kind: "acknowledged" as const, data: result.data, acknowledge: () => financeCommandFetch.acknowledge(response) };
}
