"use client";
import { useRef, useState } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { financeCommandFetch } from "@erpx/feature-finance/lib/finance-command-fetch";
import { financeDocumentFormUnchanged } from "@erpx/feature-finance/lib/document-post-control";
import { captureWorkCommandContext } from "@erpx/feature-work_projection/lib/command-context";
import { useFinanceActionAllowed } from "@erpx/feature-finance/components/purchase-context";
import { useFinanceDraftScope, useFinanceDraftCommandPending, captureFinanceForm } from "@erpx/feature-finance/components/finance-draft-boundary";

type PendingFinanceCommand = { token: object; completion: ReturnType<typeof captureFinanceForm>; uncertain: boolean };

/** Finance presentation flags are advisory. A retry retains the exact URL,
 * payload and rendered role; the owner rechecks current authority at commit. */
export function useFinanceCommand(base: string, capability: string | readonly string[] = "finance.accounting") {
  const router = useRouter(), draftScope = useFinanceDraftScope(), trackCommandPending = useFinanceDraftCommandPending(), attempts = useRef(new Map<string, PendingFinanceCommand>());
  const allowedNew = useFinanceActionAllowed("new-work", capability), allowedSettle = useFinanceActionAllowed("settle-existing", capability);
  const [pending, setPending] = useState(false), [error, setError] = useState(""), [hasAttempt, setHasAttempt] = useState(false);
  function resolveAttempt(identity: string, attempt: PendingFinanceCommand) {
    if (attempts.current.get(identity) !== attempt) return;
    attempts.current.delete(identity);
    trackCommandPending(attempt.token, attempt.completion, false);
  }
  async function send(path: string, input: unknown, purpose: "new-work" | "settle-existing" = "settle-existing") {
    const url = `${base.replace(canonicalUiHref("/holdings/"), "/api/v1/holdings/")}/finance/${path}`, body = JSON.stringify(input), identity = JSON.stringify([url, body]);
    if ([...attempts.current.keys()].some(existingIdentity => existingIdentity !== identity)) {
      setError("Retry the original Finance action before submitting a different action. Your newer draft is retained."); return null;
    }
    const retry = financeCommandFetch.hasAttempt(url, body);
    if (!(purpose === "new-work" ? allowedNew : allowedSettle) && !retry) {
      setError("This Finance action is unavailable for the current package. Your draft is retained."); return null;
    }
    const existing = attempts.current.get(identity),
      completion = existing?.completion ?? (retry ? financeCommandFetch.completion(url, body) : captureFinanceForm()),
      attempt = existing ?? { token: {}, completion, uncertain: false };
    attempts.current.set(identity, attempt);
    setPending(true); setError(""); setHasAttempt(true);
    trackCommandPending(attempt.token, attempt.completion, true);
    const assertCurrent = captureWorkCommandContext();
    try {
      const response = await financeCommandFetch(url, { method: "POST", headers: { "Content-Type": "application/json" }, body }, "caller");
      const result = await response.json();
      if (!response.ok) {
        if (!attempt.uncertain && response.status < 500 && result.error?.code !== "WORK_CONTEXT_CHANGED") resolveAttempt(identity, attempt);
        throw new Error(result.error?.message ?? "The Finance action could not be saved.");
      }
      if (result.data === undefined) throw new Error("Finance acknowledgement is missing. Retry the same action.");
      assertCurrent();
      if (!completion && captureFinanceForm()) {
        attempt.uncertain = true;
        setError("The submitted Finance action was saved. Your newer draft is retained."); return null;
      }
      if (!financeCommandFetch.acknowledge(response)) throw new Error("Finance acknowledgement could not be verified. Retry the same action.");
      resolveAttempt(identity, attempt);
      setHasAttempt(false);
      if (!financeDocumentFormUnchanged(completion)) {
        setError("The submitted Finance action was saved. Your newer draft is retained."); return null;
      }
      router.refresh(); return result.data;
    } catch (cause) {
      if (attempts.current.get(identity) === attempt) attempt.uncertain = true;
      setError(cause instanceof Error ? cause.message : "Connection interrupted. Retry the same action safely."); return null;
    } finally { setPending(false); }
  }
  return { send, pending, busy: pending, error, setError, draftScope, canPrepare: allowedNew || hasAttempt, canSettle: allowedSettle || hasAttempt };
}
