"use client";
import {useRef,useState} from "react";
import {useRouter} from "@/lib/shared/destination-router";
import {canonicalUiHref} from "@/lib/work_projection/destination-registry";
import {refundCommandFetch,validRefundAcknowledgement,type RefundCommandResult} from "@/lib/finance/refund-command-fetch";
import {refundCommandWithinSource,type RefundCommandScope} from "@/lib/finance/refund-command-controls";
import {financeDocumentFormUnchanged} from "@/lib/finance/document-post-control";
import {captureWorkCommandContext} from "@/lib/work_projection/command-context";
import {useFinanceActionAllowed} from "./purchase-context";
import {captureFinanceForm,useFinanceDraftScope} from "./finance-draft-boundary";
export function useRefundCommand(base: string, capabilities: readonly string[], scope: RefundCommandScope) {
  const router = useRouter(),draftScope = useFinanceDraftScope(),allowed = useFinanceActionAllowed("settle-existing",capabilities),history = useFinanceActionAllowed("history",capabilities),pendingRef = useRef(false);
  const [busy,setBusy] = useState(false),[error,setError] = useState(""),[notice,setNotice] = useState(""),[hasAttempt,setHasAttempt] = useState(false);
  async function run(path: string,input: unknown): Promise<RefundCommandResult | null> {
    if (pendingRef.current) return null;
    if (!refundCommandWithinSource(path,input,scope)) {setError("This action does not belong to the current refund.");return null;}
    const url = `${base.replace(canonicalUiHref("/holdings/"),"/api/v1/holdings/")}/finance/${path}`,body = JSON.stringify(input);
    const retry = refundCommandFetch.hasAttempt(url,body);
    if (!allowed && !retry) {setError("This refund action is unavailable for the current package. Your draft is retained.");return null;}
    const completion = retry ? refundCommandFetch.completion(url,body) : captureFinanceForm(),assertCurrent = captureWorkCommandContext();
    pendingRef.current = true;setBusy(true);setError("");setNotice("");setHasAttempt(true);
    try {
      const response = await refundCommandFetch(url,{method:"POST",headers:{"Content-Type":"application/json"},body},completion,"caller");
      const result = await response.json() as {data?: unknown;error?:{message?:string}};
      if (!response.ok) throw Error(result.error?.message ?? "The refund action could not be saved.");
      if (!validRefundAcknowledgement(url,result.data)) throw Error("Refund acknowledgement is incomplete. Retry the same action.");
      assertCurrent();
      if (!completion && captureFinanceForm()) {setNotice("Submitted refund change saved. Your newer draft is retained.");return null;}
      if (!refundCommandFetch.acknowledge(response)) throw Error("Refund acknowledgement could not be verified. Retry the same action.");
      setHasAttempt(false);
      if (!financeDocumentFormUnchanged(completion)) {setNotice("Submitted refund change saved. Your newer draft is retained.");return null;}
      router.refresh();return result.data;
    } catch (cause) {setError(cause instanceof Error ? cause.message : "Connection interrupted. Retry the same refund action safely.");return null;}
    finally {pendingRef.current = false;setBusy(false);}
  }
  return {run,busy,error,setError,notice,draftScope,canSettle:allowed || hasAttempt,canReadHistory:history};
}
