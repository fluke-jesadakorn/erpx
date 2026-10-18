"use client";
import { useState } from "react";
import { useRouter } from "@/lib/shared/destination-router";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { financeCommandFetch } from "@/lib/finance/finance-command-fetch";
import { financeDocumentFormUnchanged } from "@/lib/finance/document-post-control";
import { captureWorkCommandContext } from "@/lib/work_projection/command-context";
import { useFinanceActionAllowed } from "./purchase-context";
import { useFinanceDraftScope, captureFinanceForm } from "./finance-draft-boundary";

/** Finance presentation flags are advisory. A retry retains the exact URL,
 * payload and rendered role; the owner rechecks current authority at commit. */
export function useFinanceCommand(base: string, capability: string | readonly string[] = "finance.accounting") {
  const router = useRouter(), draftScope = useFinanceDraftScope();
  const allowedNew = useFinanceActionAllowed("new-work", capability), allowedSettle = useFinanceActionAllowed("settle-existing", capability);
  const [pending, setPending] = useState(false), [error, setError] = useState(""), [hasAttempt, setHasAttempt] = useState(false);
  async function send(path: string, input: unknown, purpose: "new-work" | "settle-existing" = "settle-existing") {
    const url = `${base.replace(canonicalUiHref("/holdings/"), "/api/v1/holdings/")}/finance/${path}`, body = JSON.stringify(input);
    const retry = financeCommandFetch.hasAttempt(url, body);
    if (!(purpose === "new-work" ? allowedNew : allowedSettle) && !retry) {
      setError("This Finance action is unavailable for the current package. Your draft is retained."); return null;
    }
    const completion = retry ? financeCommandFetch.completion(url, body) : captureFinanceForm(), assertCurrent = captureWorkCommandContext();
    setPending(true); setError(""); setHasAttempt(true);
    try {
      const response = await financeCommandFetch(url, { method: "POST", headers: { "Content-Type": "application/json" }, body }, "caller");
      const result = await response.json();
      if (!response.ok) throw new Error(result.error?.message ?? "The Finance action could not be saved.");
      if (result.data === undefined) throw new Error("Finance acknowledgement is missing. Retry the same action.");
      assertCurrent();
      if (!completion && captureFinanceForm()) {
        setError("The submitted Finance action was saved. Your newer draft is retained."); return null;
      }
      if (!financeCommandFetch.acknowledge(response)) throw new Error("Finance acknowledgement could not be verified. Retry the same action.");
      setHasAttempt(false);
      if (!financeDocumentFormUnchanged(completion)) {
        setError("The submitted Finance action was saved. Your newer draft is retained."); return null;
      }
      router.refresh(); return result.data;
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Connection interrupted. Retry the same action safely."); return null;
    } finally { setPending(false); }
  }
  return { send, pending, busy: pending, error, setError, draftScope, canPrepare: allowedNew || hasAttempt, canSettle: allowedSettle || hasAttempt };
}
