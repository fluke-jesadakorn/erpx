import {workFetch, type WorkCommandCompletion} from "@/lib/work_projection/work-fetch";
import {announceWorkChange} from "@/lib/work_projection/work-refresh";
import {financeDocumentCommandContext} from "./document-post-control";
import {captureWorkCommandContext} from "@/lib/work_projection/command-context";
import {retainWorkCommandCompletion} from "@/lib/work_projection/command-completion";
export type RefundCommandResult = {id: string; content?: string; sha256?: string};
export function validRefundAcknowledgement(url: string, value: unknown): value is RefundCommandResult {
  if (!value || typeof value !== "object" || Array.isArray(value) || !("id" in value) || typeof value.id !== "string" || !value.id) return false;
  return !url.endsWith("/refund-instructions/export") || "content" in value && typeof value.content === "string" && !!value.content && "sha256" in value && typeof value.sha256 === "string" && !!value.sha256;
}
/** Uncertain refunds keep their exact command key until the owner returns its
 * parsed receipt. An export additionally needs its retained content and hash. */
export function createRefundCommandFetch(send: typeof workFetch = workFetch) {
  const attempts = new Map<string,{key:string;completion:ReturnType<typeof retainWorkCommandCompletion>}>();
  const identity = (url: string,body: BodyInit | null | undefined) => JSON.stringify([financeDocumentCommandContext(),url,body ?? null]);
  // Callers that decode the original response defer local completion until
  // their own receipt/context check; a clone alone cannot release the key.
  const receipts = new WeakMap<Response, () => boolean>();
  const acknowledge = (response:Response)=>receipts.get(response)?.()??false;
  const request = async (url: string, init: RequestInit, completion?: WorkCommandCompletion, acknowledgement: "automatic" | "caller" = "automatic") => {
    const signature = identity(url,init.body), headers = new Headers(init.headers);
    let attempt = attempts.get(signature);
    if (!attempt) {attempt={key:crypto.randomUUID(),completion:retainWorkCommandCompletion(completion)};attempts.set(signature,attempt);}
    headers.set("Idempotency-Key",attempt.key);
    const originalCompletion=attempt.completion(),assertCurrent=captureWorkCommandContext();
    const response = await send(url,{...init,headers},originalCompletion,"caller");
    if (response.ok && validRefundAcknowledgement(url,(await response.clone().json().catch(()=>null))?.data)) {
      assertCurrent();
      if(signature!==identity(url,init.body))throw Object.assign(new Error('Return to the original refund command before retrying.'),{code:'WORK_CONTEXT_CHANGED'});
      const retainedAttempt=attempt;
      receipts.set(response,()=>{
        assertCurrent();
        if(signature!==identity(url,init.body))throw Object.assign(new Error('Return to the original refund command before retrying.'),{code:'WORK_CONTEXT_CHANGED'});
        if(attempts.get(signature)!==retainedAttempt)return false;
        attempts.delete(signature);
        if(typeof window!=="undefined") {
          const originalCompletion=retainedAttempt.completion();
          if(originalCompletion){
            window.dispatchEvent(new CustomEvent("erpx:finance-command-acknowledged",{detail:originalCompletion}));
            window.dispatchEvent(new CustomEvent("erpx:record-command-saved",{detail:{...originalCompletion,financeAcknowledged:true}}));
          }
          announceWorkChange();
        }
        receipts.delete(response);
        return true;
      });
      if(acknowledgement==="automatic")acknowledge(response);
    }
    return response;
  };
  request.acknowledge = acknowledge;
  request.hasAttempt = (url: string,body: BodyInit | null | undefined) => attempts.has(identity(url,body));
  request.completion = (url:string,body:BodyInit|null|undefined)=>attempts.get(identity(url,body))?.completion();
  return request;
}
export const refundCommandFetch = createRefundCommandFetch();
