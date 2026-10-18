import { workFetch } from "@/lib/work_projection/work-fetch";
import { announceWorkChange } from "@/lib/work_projection/work-refresh";
import {captureWorkCommandContext} from "@/lib/work_projection/command-context";
import {retainWorkCommandCompletion} from "@/lib/work_projection/command-completion";

/** Keep the command identity after an uncertain response, scoped to the rendered work role. */
export function createFinanceCommandFetch(send: typeof workFetch = workFetch) {
  const attempts = new Map<string, {key:string;completion:ReturnType<typeof retainWorkCommandCompletion>}>();
  function attemptSignature(input:RequestInfo|URL,method:string,body:string) {
    const context = typeof document === "undefined" ? null : document.querySelector<HTMLElement>("[data-work-revision]");
    const stamp = context ? [context.dataset.workUser, context.dataset.workRevision, context.dataset.workCompany, context.dataset.workAssignment] : [];
    return JSON.stringify([input instanceof Request ? input.url : String(input), method, stamp, body]);
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
    let attempt = attempts.get(signature);
    if (!attempt) {
      const form=typeof document==="undefined"?null:document.activeElement?.closest<HTMLFormElement>("form");
      attempt={key:crypto.randomUUID(),completion:retainWorkCommandCompletion(form?{form,formEditVersion:form.getAttribute("data-erpx-edit-version")}:undefined)};
      attempts.set(signature, attempt);
    }
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
          if(attempts.get(signature)!==retainedAttempt)return false;
          attempts.delete(signature);
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
    completion:(input:RequestInfo|URL,body:string,method="POST")=>attempts.get(attemptSignature(input,method,body))?.completion(),
  });
}

export const financeCommandFetch = createFinanceCommandFetch();
