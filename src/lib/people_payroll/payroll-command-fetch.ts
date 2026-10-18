import { workFetch, type WorkCommandCompletion } from "@/lib/work_projection/work-fetch";
import { captureWorkCommandContext } from "@/lib/work_projection/command-context";
import { retainWorkCommandCompletion } from "@/lib/work_projection/command-completion";
import { announceWorkChange } from "@/lib/work_projection/work-refresh";

export function payrollCommandContext() {
  const node = typeof document === "undefined" ? null : document.querySelector<HTMLElement>("[data-work-revision]");
  return node ? [node.dataset.workUser, node.dataset.workRevision, node.dataset.workCompany, node.dataset.workAssignment].join(":") : "server";
}
export function capturePayrollForm(): WorkCommandCompletion | undefined {
  const form = typeof document === "undefined" ? null : document.activeElement?.closest<HTMLFormElement>("form[data-payroll-draft]");
  return form ? {form, formEditVersion: form.getAttribute("data-erpx-edit-version")} : undefined;
}
export function payrollFormUnchanged(completion: WorkCommandCompletion | undefined) {
  return !completion || completion.form.isConnected && completion.form.getAttribute("data-erpx-edit-version") === completion.formEditVersion;
}
export function validPayrollAcknowledgement(value: unknown): value is Record<string, unknown> & {id: string} {
  return !!value && typeof value === "object" && !Array.isArray(value) && "id" in value && typeof value.id === "string" && value.id.length > 0;
}
/** Retain exact command identity through network loss or incomplete 2xx
 * acknowledgement. Contexts and changed payloads have separate identities. */
export function createPayrollCommandFetch(send: typeof workFetch = workFetch) {
  const attempts = new Map<string, {key: string; completion: ReturnType<typeof retainWorkCommandCompletion>}>();
  const receipts = new WeakMap<Response, (value?: unknown) => boolean>();
  const identity = (url: string, body: BodyInit | null | undefined) => JSON.stringify([payrollCommandContext(), url, body ?? null]);
  const acknowledge = (response: Response, value?: unknown) => receipts.get(response)?.(value) ?? false;
  const request = async (url: string, init: RequestInit, completion = capturePayrollForm(), acknowledgement: "automatic" | "caller" = "automatic") => {
    const id = identity(url, init.body), headers = new Headers(init.headers);
    let attempt = attempts.get(id);
    if (!attempt) {attempt = {key: crypto.randomUUID(), completion: retainWorkCommandCompletion(completion)}; attempts.set(id, attempt);}
    const retained = attempt, guard = captureWorkCommandContext();
    guard(); headers.set("Idempotency-Key", retained.key);
    const response = await send(url, {...init, headers}, retained.completion(), "caller");
    if (response.ok) {
      const value: unknown = (await response.clone().json().catch(() => null))?.data;
      if (validPayrollAcknowledgement(value)) {
        receipts.set(response, decoded => {
          guard();
          if (id !== identity(url, init.body)) throw Object.assign(Error("Return to the original Payroll work context before retrying."), {code: "WORK_CONTEXT_CHANGED"});
          if (attempts.get(id) !== retained || decoded !== undefined && (!validPayrollAcknowledgement(decoded) || JSON.stringify(decoded) !== JSON.stringify(value))) return false;
          attempts.delete(id); receipts.delete(response);
          if (typeof window !== "undefined") {
            const original = retained.completion();
            if (original) window.dispatchEvent(new CustomEvent("erpx:record-command-saved", {detail: {...original, payrollAcknowledged: true}}));
            announceWorkChange();
          }
          return true;
        });
        if (acknowledgement === "automatic") acknowledge(response);
      }
    }
    return response;
  };
  request.acknowledge = acknowledge;
  request.hasAttempt = (url: string, body: BodyInit | null | undefined) => attempts.has(identity(url, body));
  return request;
}
export const payrollCommandFetch = createPayrollCommandFetch();
