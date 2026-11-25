import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {readCompanyPurchaseFacts} from "@erpx/server-commercial/public/package-grants";
import {purchasedCapability} from "@erpx/server-commercial/public/rules/packages";
import {nativeInboxNotificationsSSR,nativeInboxSummarySSR} from "@erpx/server-collaboration/public/native-inbox-page";

/** SSR inbox snapshot served only by the native Collaboration read port.
 * The retained TS inboxPage/inboxSummary calls are deleted: no availability
 * gate and no TS fallback remain here. Missing native transport fails closed
 * (503) inside the native client. Only owner-resolved notifications cross the
 * Client boundary; product-only foundation never preloads conversations. */
export async function notificationWorkspace(actor:WorkContext,productHost:boolean){
 const [page,summary,facts]=await Promise.all([
  nativeInboxNotificationsSSR(actor.holdingSlug,"?view=unread"),
  nativeInboxSummarySSR(actor.holdingSlug),
  (async()=>{
   if(!productHost)return null;
   const {write}=await holdingResources(actor.holdingId);
   return readCompanyPurchaseFacts(write,actor.companyId);
  })(),
 ]);
 const notificationsOnly=!!facts&&!purchasedCapability(facts.grants,actor.companyId,"work.today","history",Date.now(),facts.history).allowed;
 return {initial:{items:page.items,nextCursor:page.nextCursor,unread:summary.unread},notificationsOnly};
}
