import "server-only";
import type {WorkContext} from "../../identity_access/public/authorization";
import {holdingResources} from "../../runtime/public/runtime";
import {readCompanyPurchaseFacts} from "../../commercial/public/package-grants";
import {purchasedCapability} from "../../commercial/public/rules/packages";
import {inboxPage,inboxSummary} from "./inbox";

/** Only owner-resolved notifications cross the Client boundary. Product-only
 * foundation entry never preloads conversations, people or sent notices. */
export async function notificationWorkspace(actor:WorkContext,productHost:boolean){
 const {write}=await holdingResources(actor.holdingId);
 const [page,summary,facts]=await Promise.all([inboxPage(write,actor,{view:"unread"}),inboxSummary(write,actor),productHost?readCompanyPurchaseFacts(write,actor.companyId):Promise.resolve(null)]);
 const notificationsOnly=!!facts&&!purchasedCapability(facts.grants,actor.companyId,"work.today","history",Date.now(),facts.history).allowed;
 return {initial:{items:page.items,nextCursor:page.nextCursor,unread:summary.unread},notificationsOnly};
}
