"use client";
import {useEffect, useLayoutEffect, useRef, useState} from "react";
import {useRouter} from "@/lib/shared/destination-router";
import {canonicalUiHref} from "@/lib/work_projection/destination-registry";
import {captureWorkCommandContext} from "@/lib/work_projection/command-context";
import {retainWorkCommandCompletion} from "@/lib/work_projection/command-completion";
import {contextIsSwitching, registerUnsavedWork} from "@/lib/work_projection/context-switch";
import {capturePayrollForm, payrollCommandContext, payrollCommandFetch, payrollFormUnchanged, validPayrollAcknowledgement} from "@/lib/people_payroll/payroll-command-fetch";
import type {WorkCommandCompletion} from "@/lib/work_projection/work-fetch";
import {payrollActionAllowed, payrollCommandPurpose} from "@/lib/people_payroll/payroll-purchases";
import {payrollErrorMessage, type PayrollApiError} from "@/lib/people_payroll/payroll-errors";
import {usePayrollPurchase} from "./payroll-purchase-context";

type Attempt = {base: string; ownerKey: string; context: string; path: string; body: string; redirect: boolean; uncertain: boolean; hadCompletion: boolean; completion: ReturnType<typeof retainWorkCommandCompletion>};

/** A retry retains the first request. Each invocation fences its current SSR
 * source and purchase snapshot; a later history snapshot may admit exact replay. */
export function usePayrollCommand(base: string, ownerKey = "", source = "") {
  const router = useRouter(), purchase = usePayrollPurchase(), pending = useRef(false), attempt = useRef<Attempt | null>(null);
  const live = useRef<{base: string; ownerKey: string; source: string; purchase: typeof purchase} | null>({base, ownerKey, source, purchase});
  useLayoutEffect(() => {live.current = {base, ownerKey, source, purchase}; return () => {live.current = null;};}, [base, ownerKey, source, purchase]);
  useEffect(() => {
    const dirty = () => attempt.current !== null, unregister = registerUnsavedWork(dirty);
    const unload = (event: BeforeUnloadEvent) => {if (dirty() && !contextIsSwitching()) event.preventDefault();};
    window.addEventListener("beforeunload", unload);
    return () => {unregister(); window.removeEventListener("beforeunload", unload);};
  }, []);
  const [busy, setBusy] = useState(false), [error, setError] = useState(""), [notice, setNotice] = useState(""), [hasAttempt, setHasAttempt] = useState(false);
  async function execute(captured: Attempt, retry = false) {
    if (pending.current) return false;
    const current = live.current;
    if (!current || current.base !== captured.base || current.ownerKey !== captured.ownerKey || payrollCommandContext() !== captured.context || !payrollActionAllowed(current.purchase, retry ? "history" : payrollCommandPurpose(captured.path, JSON.parse(captured.body)))) {
      setError("Return to the original Payroll owner and current work role before retrying. Your request and newer draft are retained."); return false;
    }
    const stamp = JSON.stringify([current.source, current.purchase]), work = captureWorkCommandContext();
    const assertCurrent = () => {
      work(); const now = live.current;
      if (!now || now.base !== captured.base || now.ownerKey !== captured.ownerKey || payrollCommandContext() !== captured.context || JSON.stringify([now.source, now.purchase]) !== stamp)
        throw Object.assign(Error("The Payroll source or current work role changed. Retry the original request after reviewing the current owner."), {code: "WORK_CONTEXT_CHANGED"});
    };
    try {assertCurrent();} catch (cause) {setError(payrollErrorMessage(cause)); return false;}
    const wasUncertain = captured.uncertain;
    attempt.current = captured; pending.current = true; setBusy(true); setError(""); setNotice(""); setHasAttempt(true);
    let mayHaveCommitted = true;
    try {
      const response = await payrollCommandFetch(`/api/v1${captured.base}/people/${captured.path}`, {method: "POST", headers: {"Content-Type": "application/json"}, body: captured.body}, captured.completion(), "caller");
      const result = await response.json() as {data?: unknown; error?: PayrollApiError};
      assertCurrent();
      if (!response.ok) {
        mayHaveCommitted = wasUncertain || response.status >= 500 || result.error?.code === "WORK_CONTEXT_CHANGED";
        if (!mayHaveCommitted) {attempt.current = null; setHasAttempt(false);}
        throw result.error ?? {message: "Could not save."};
      }
      if (!validPayrollAcknowledgement(result.data) || !payrollCommandFetch.acknowledge(response, result.data)) throw Error("The Payroll acknowledgement is incomplete. Retry the original request.");
      mayHaveCommitted = false; attempt.current = null; setHasAttempt(false);
      const completion = captured.completion(), unchanged = captured.hadCompletion ? !!completion && payrollFormUnchanged(completion) : !capturePayrollForm();
      if (!unchanged) {setNotice("Submitted change saved. Your newer draft is retained."); return false;}
      if (captured.redirect) router.push(canonicalUiHref(`${captured.base}/people/payroll/runs/${result.data.id}`));
      router.refresh(); return true;
    } catch (cause) {
      if (mayHaveCommitted) {captured.uncertain = true; attempt.current = captured; setHasAttempt(true);}
      setError(payrollErrorMessage(cause, "Could not save Payroll changes.")); return false;
    } finally {pending.current = false; setBusy(false);}
  }
  function send(path: string, input: unknown, redirect = false, suppliedCompletion?: WorkCommandCompletion) {
    if (pending.current) return Promise.resolve(false);
    // Newer form submissions never replace an unresolved first request.
    if (attempt.current) {setError("Recover the original Payroll request first. Your newer draft is retained."); return Promise.resolve(false);}
    try {
      const body = JSON.stringify(input);
      if (typeof body !== "string") throw Error("Review the Payroll request before submitting.");
      const completion = suppliedCompletion ?? capturePayrollForm();
      return execute({base, ownerKey, context: payrollCommandContext(), path, body, redirect, uncertain: false, hadCompletion: !!completion, completion: retainWorkCommandCompletion(completion)});
    } catch (cause) {setError(payrollErrorMessage(cause)); return Promise.resolve(false);}
  }
  const retry = () => attempt.current ? execute(attempt.current, true) : Promise.resolve(false);
  return {send, retry, busy, error, setError, notice, hasAttempt, recovery: {pending: hasAttempt, busy, retry},
    canPrepare: payrollActionAllowed(purchase, "new-work") || hasAttempt,
    canSettle: payrollActionAllowed(purchase, "settle-existing") || hasAttempt,
    canRead: payrollActionAllowed(purchase, "history")};
}
