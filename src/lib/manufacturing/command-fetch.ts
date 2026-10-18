import { workFetch, type WorkCommandCompletion } from "@/lib/work_projection/work-fetch";
import { announceWorkChange } from "@/lib/work_projection/work-refresh";
import { captureWorkCommandContext } from "@/lib/work_projection/command-context";
import { retainWorkCommandCompletion } from "@/lib/work_projection/command-completion";

export function manufacturingCommandContext() {
  const node = typeof document === "undefined" ? null : document.querySelector<HTMLElement>("[data-work-revision]");
  return node ? [node.dataset.workUser, node.dataset.workRevision, node.dataset.workCompany, node.dataset.workAssignment].join(":") : "server";
}
export function captureManufacturingForm(): WorkCommandCompletion | undefined {
  const form = typeof document === "undefined" ? null : document.activeElement?.closest<HTMLFormElement>("form[data-manufacturing-draft]");
  return form ? {form, formEditVersion: form.getAttribute("data-erpx-edit-version")} : undefined;
}
export function manufacturingFormUnchanged(completion: WorkCommandCompletion | undefined) {
  return !completion || completion.form.isConnected && completion.form.getAttribute("data-erpx-edit-version") === completion.formEditVersion;
}
export function validManufacturingAcknowledgement(value: unknown,request?:unknown): value is Record<string, unknown> & {id: string;version:number} {
  if(!value || typeof value !== "object" || Array.isArray(value) || !("id" in value) || typeof value.id !== "string" || !value.id.length || !("version" in value) || !Number.isSafeInteger(value.version) || Number(value.version)<=0)return false;
  if(request&&typeof request==="object"&&!Array.isArray(request)&&"entity" in request&&request.entity==="order"&&"action" in request&&request.action==="cancel")return "id" in request&&typeof request.id==="string"&&"expectedVersion" in request&&Number.isSafeInteger(request.expectedVersion)&&Number(request.expectedVersion)>0&&value.id===request.id&&"status" in value&&value.status==="cancelled"&&value.version===Number(request.expectedVersion)+1;
  return true;
}
function submittedCommand(body:BodyInit|null|undefined):unknown{if(typeof body!=="string")return undefined;try{return JSON.parse(body);}catch{return undefined;}}
/** Retain exact command identity through network loss or incomplete 2xx
 * acknowledgement. Contexts and changed payloads have separate identities. */
export function createManufacturingCommandFetch(send: typeof workFetch = workFetch) {
  const attempts = new Map<string, {key:string;completion:ReturnType<typeof retainWorkCommandCompletion>}>();
  const identity = (url: string, body: BodyInit | null | undefined) => JSON.stringify([manufacturingCommandContext(), url, body ?? null]);
  const receipts = new WeakMap<Response, () => boolean>();
  const acknowledge = (response: Response) => receipts.get(response)?.() ?? false;
  const request = async (url: string, init: RequestInit, completion = captureManufacturingForm(), acknowledgement: "automatic" | "caller" = "automatic") => {
    const body=init.body,submitted=submittedCommand(body),id = identity(url, body), headers = new Headers(init.headers);
    let attempt = attempts.get(id);
    if (!attempt) {attempt = {key:crypto.randomUUID(),completion:retainWorkCommandCompletion(completion)}; attempts.set(id, attempt);}
    const originalCompletion=attempt.completion();headers.set("Idempotency-Key", attempt.key);
    const assertCurrent=captureWorkCommandContext(),response = await send(url, {...init, body, headers}, originalCompletion, "caller");
    if (response.ok) {
      const acknowledged = validManufacturingAcknowledgement((await response.clone().json().catch(() => null))?.data,submitted);
      if (acknowledged) {
        assertCurrent();if(id!==identity(url,body))throw Object.assign(new Error("Return to the original Manufacturing context before retrying."),{code:"WORK_CONTEXT_CHANGED"});
        const retainedAttempt=attempt;
        receipts.set(response,()=>{
          assertCurrent();if(id!==identity(url,body))throw Object.assign(new Error("Return to the original Manufacturing context before retrying."),{code:"WORK_CONTEXT_CHANGED"});
          if(attempts.get(id)!==retainedAttempt)return false;
          attempts.delete(id);
          if (typeof window !== "undefined") {
            const capturedCompletion=retainedAttempt.completion();
            if (capturedCompletion) window.dispatchEvent(new CustomEvent("erpx:record-command-saved", {detail: {...capturedCompletion, manufacturingAcknowledged: true}}));
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
  return request;
}
export const manufacturingCommandFetch = createManufacturingCommandFetch();
