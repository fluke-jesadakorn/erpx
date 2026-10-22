"use client";
import {useRef, useState} from "react";
import {useRouter} from "@erpx/feature-shared/lib/destination-router";
import {useEmbeddedRecord} from "@erpx/ui/components/record-context";
import {canonicalUiHref} from "@erpx/feature-work_projection/lib/destination-registry";
import {recordDrawerHref, recordTarget} from "@erpx/feature-records/lib/record-drawer";
import {hubResultHref} from "@erpx/feature-shared/lib/hub-result";
import {captureWorkCommandContext} from "@erpx/feature-work_projection/lib/command-context";
import {captureCommerceForm, commerceCommandFetch, commerceFormUnchanged, validCommerceAcknowledgement} from "@erpx/feature-commerce/lib/commerce-command-fetch";
import {commerceCommandAllowed} from "@erpx/feature-commerce/lib/purchase-controls";
import {useCommercePurchase} from "@erpx/feature-commerce/components/purchase-context";

export function useCommerceCommand(base: string) {
  const router = useRouter(), embedded = useEmbeddedRecord(), purchase = useCommercePurchase(), pendingRef = useRef(false);
  const [pending, setPending] = useState(false), [error, setError] = useState(""), [notice, setNotice] = useState(""), [hasAttempt, setHasAttempt] = useState(false);
  async function send(path: string, input: unknown, target?: string) {
    if (pendingRef.current) return false;
    const body = JSON.stringify(input), url = `${base.replace(canonicalUiHref("/holdings/"), "/api/v1/holdings/")}/commerce/${path}`, completion = commerceCommandFetch.hasAttempt(url,body) ? commerceCommandFetch.completion(url,body) : captureCommerceForm();
    if (!commerceCommandAllowed(purchase, path, input) && !commerceCommandFetch.hasAttempt(url, body)) {
      setError("This document action is unavailable for the current package. Your draft is retained."); return false;
    }
    pendingRef.current = true; setPending(true); setError(""); setNotice(""); setHasAttempt(true);
    try {
      const assertCurrent=captureWorkCommandContext(),response = await commerceCommandFetch(url, {method: "POST", headers: {"Content-Type": "application/json"}, body}, completion, "caller");
      const result = await response.json() as {data?: {id?: string; salesId?: string; orderId?: string}; error?: {message?: string}};
      if (!response.ok) throw Error(result.error?.message ?? "Unable to save.");
      if (!validCommerceAcknowledgement(result.data)) throw Error("The document acknowledgement is incomplete. Retry the same action.");
      assertCurrent();
      if (!commerceCommandFetch.acknowledge(response)) throw Error("The document acknowledgement is incomplete. Retry the same action.");
      setHasAttempt(false);
      if (!commerceFormUnchanged(completion) || !completion && captureCommerceForm()) {setNotice("Submitted change saved. Your newer draft is retained."); return false;}
      const destination = target ?? canonicalUiHref(`${base}/commerce/orders/${result.data.salesId ?? result.data.orderId ?? result.data.id}`);
      const record = recordTarget(destination, base), hubResult = hubResultHref(window.location.href, base, destination);
      window.dispatchEvent(new CustomEvent("erpx:hub-saved", {detail: completion}));
      window.dispatchEvent(new CustomEvent("erpx:record-saved", {detail: completion}));
      if (hubResult) router.replace(hubResult, {scroll: false});
      else if (embedded && record) router.replace(recordDrawerHref(window.location.href, record), {scroll: false});
      else if (!embedded) router.push(destination);
      router.refresh(); return true;
    } catch (cause) {setError(cause instanceof Error ? cause.message : "Unable to save."); return false;}
    finally {pendingRef.current = false; setPending(false);}
  }
  return {send, pending, error, notice, hasAttempt, purchase,
    canPrepare: purchase?.newWork !== false || hasAttempt, canSettle: purchase?.settleExisting !== false || hasAttempt};
}
