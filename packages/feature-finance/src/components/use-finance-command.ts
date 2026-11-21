"use client";
import { useRef, useState } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { financeCommandFetch, submitFinanceCommand } from "@erpx/feature-finance/public/client";
import { createFinanceSubmission } from "../application/submit-command";
import type { PendingFinanceSubmission } from "../domain/submission";
import { financeDocumentFormUnchanged } from "@erpx/feature-finance/lib/document-post-control";
import { captureWorkCommandContext } from "@erpx/feature-work_projection/lib/command-context";
import { useFinanceActionAllowed } from "@erpx/feature-finance/components/purchase-context";
import { useFinanceDraftScope, useFinanceDraftCommandPending, captureFinanceForm } from "@erpx/feature-finance/components/finance-draft-boundary";

type PendingFinanceCommand = PendingFinanceSubmission<ReturnType<typeof captureFinanceForm>>;

/** Finance presentation flags are advisory. A retry retains the exact URL,
 * payload and rendered role; the owner rechecks current authority at commit. */
export function useFinanceCommand(base: string, capability: string | readonly string[] = "finance.accounting") {
  const router = useRouter(), draftScope = useFinanceDraftScope(), trackCommandPending = useFinanceDraftCommandPending(), attempts = useRef(new Map<string, PendingFinanceCommand>());
  const allowedNew = useFinanceActionAllowed("new-work", capability), allowedSettle = useFinanceActionAllowed("settle-existing", capability);
  const [pending, setPending] = useState(false), [error, setError] = useState(""), [hasAttempt, setHasAttempt] = useState(false);
  function send(path: string, input: unknown, purpose: "new-work" | "settle-existing" = "settle-existing") {
    const submit = createFinanceSubmission(attempts.current, {
      hasAttempt: input => financeCommandFetch.hasAttempt(input.target, input.body),
      retainedCompletion: input => financeCommandFetch.completion(input.target, input.body),
      captureCompletion: captureFinanceForm,
      canBegin: purpose => purpose === "new-work" ? allowedNew : allowedSettle,
      trackPending: trackCommandPending,
      captureContext: captureWorkCommandContext,
      submit: submitFinanceCommand,
      completionUnchanged: financeDocumentFormUnchanged,
      refresh: () => router.refresh(),
      setPending,
      setHasAttempt,
      setError,
    });
    const target = `${base.replace(canonicalUiHref("/holdings/"), "/api/v1/holdings/")}/finance/${path}`;
    return submit({ target, body: JSON.stringify(input) }, purpose);
  }
  return { send, pending, busy: pending, error, setError, draftScope, canPrepare: allowedNew || hasAttempt, canSettle: allowedSettle || hasAttempt };
}
