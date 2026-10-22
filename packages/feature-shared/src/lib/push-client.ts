"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";

import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import type { CurrentWorkspaceChoice } from "@erpx/feature-work_projection/lib/workspace-choice";

export function supportsPush() {
  return typeof window !== "undefined" && window.isSecureContext &&
    "serviceWorker" in navigator && "PushManager" in window && "Notification" in window;
}
export function applicationServerKey(value:string) {
  const base64=value.replace(/-/g,"+").replace(/_/g,"/");
  const bytes=atob(base64.padEnd(Math.ceil(base64.length/4)*4,"="));
  return Uint8Array.from(bytes,character=>character.charCodeAt(0));
}
export async function currentPushSubscription() {
  if(!supportsPush())return null;
  const registration=await navigator.serviceWorker.getRegistration("/");
  return registration?.pushManager.getSubscription()??null;
}
/** Called while the old account's cookie and company context still exist. */
export async function leavePushSession(base:string) {
  if(!supportsPush())return;
  if (!document.querySelector('[data-work-revision]') && base.startsWith(canonicalUiHref('/holdings/'))) {
    await leavePushHoldingSession(decodeURIComponent(base.split('/')[2]));
    return;
  }
  const subscription=await currentPushSubscription();
  const response=await workFetch(`/api/v1${base}/push`,{
    method:"DELETE",headers:{"Content-Type":"application/json"},
    body:JSON.stringify({endpoint:subscription?.endpoint}),
  });
  if(!response.ok)throw new Error("Could not turn off push for this account. Please try again.");
  await subscription?.unsubscribe();
}

async function leavePushHoldingSession(holdingSlug:string) {
  const response=await fetch(`/api/v1/holdings/${encodeURIComponent(holdingSlug)}/push/session`,
    {method:"DELETE"});
  if(!response.ok)throw new Error("Could not turn off Push for the previous workspace. Retry the switch.");
  if(supportsPush())try {await (await currentPushSubscription())?.unsubscribe();}
  catch { /* The server no longer has a subscription to send to this endpoint. */ }
}

/** The browser Push endpoint belongs to one active workspace context at a time. */
export async function leavePushForWorkspaceChange(
  current:CurrentWorkspaceChoice,
  target:{holdingSlug:string|null;companyId:string|null},
  cleanup:(holdingSlug:string)=>Promise<void>=leavePushHoldingSession,
) {
  if(current.kind==="platform" ||
    (current.holdingSlug===target.holdingSlug && current.companyId===target.companyId))return;
  await cleanup(current.holdingSlug);
}
