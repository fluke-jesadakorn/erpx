import 'server-only';
import {createHash} from 'node:crypto';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import {samePerson} from '@erpx/identity-access-ui/lib/iam';
import {requireQueuedCommandAuthority} from '@erpx/identity-access-server/public/queued-command-authority';
import {commercePackageEnforcementMode,checkPurchasedCapability} from '@erpx/commercial-server/public/purchase-command-boundary';
import {holdingResources} from '../../../../../platform/runtime/server/src/public/runtime';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {nativeSalesReadFirst,SALES_RD_NOTIFICATION_ATTEMPT,SALES_RD_NOTIFICATION_RECONCILIATION_INSTALLED,SALES_RD_NOTIFICATION_SOURCE_DEV,SALES_RD_NOTIFICATION_SOURCE_PROD} from "./native-sales-read";
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import type {CustomerMail} from '@erpx/sales-server/domain/notifications';
/** Retained read-only notice projections. Settings/queue/retry are native-owned
 * commands; this module supplies the original source witness and the
 * presentation flag for the native reconciliation reviewer. */
export type SalesNotificationSource=CustomerMail&{version:number;status:string;created_by:string;provider_message_id:string|null;lease_until:number|null;next_attempt_at:number;branch_id:string|null;department_id:string|null;order_state:string|null;customer_status:string|null;notification_email:string|null;notification_consent:string|null;current_adapter:string|null;source_company:string|null;customer_company:string|null;release_ready:boolean;image:string};
const digest=(image:string)=>createHash('sha256').update(image).digest('hex');
export const salesNotificationSourceHash=(source:SalesNotificationSource)=>digest(source.image);
/** Actual same-Company fulfillment and independently managed customer, never a
 * supplied Sales label or a private snapshot sent to the browser. */
/** Owner-local notice source read. The axum command-plan registry owns both
 * environment variants of the statement; the caller sends only the notice id. */
export function salesNotificationSource(database:SqlDatabase,id:string){
 const kind=process.env.NODE_ENV==='production'?SALES_RD_NOTIFICATION_SOURCE_PROD:SALES_RD_NOTIFICATION_SOURCE_DEV;
 return nativeSalesReadFirst<SalesNotificationSource>(kind,[id],database);}
export async function requireSalesNotificationRead(database:SqlDatabase,actor:WorkContext){const mode=await commercePackageEnforcementMode();if(mode==='disabled')return;await requireQueuedCommandAuthority(database,actor,'commerce.read');await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'sales.orders',purpose:'history'});}

/** Minimal presentation flags after the original Sales record reader. No
 * provider response, private message or capability snapshot crosses the
 * server/client boundary. Reconciliation itself is native-owned; this retained
 * read only advertises whether the current reviewer may invoke it. */
export async function customerNotificationReconciliationControls(actor:WorkContext,id:string){
 const mode=await commercePackageEnforcementMode();if(mode==='disabled')return undefined;const database=(await holdingResources(actor.holdingId)).write;await requireQueuedCommandAuthority(database,actor,'commerce.read');await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'sales.orders',purpose:'history'});const source=await salesNotificationSource(database,id);
 if(!source||source.company_id!==actor.companyId||source.source_company!==actor.companyId||source.customer_company!==actor.companyId||actor.branchId&&actor.branchId!==source.branch_id||actor.departmentId&&actor.departmentId!==source.department_id)throw new ApiError(404,'CUSTOMER_NOTIFICATION_NOT_FOUND','The original customer notice is unavailable in this work scope.');
 const attempt=source.status==='unknown'?await nativeSalesReadFirst<{id:string;source_hash:string;provider_message_id:string|null;actor_id:string}>(SALES_RD_NOTIFICATION_ATTEMPT,[id,actor.companyId,source.attempts],database):null;
 if(!attempt)return undefined;const hasProviderIdentity=source.adapter==='resend'&&!!attempt.provider_message_id&&attempt.provider_message_id===source.provider_message_id&&attempt.source_hash===salesNotificationSourceHash(source);let canReview=false;
 if(hasProviderIdentity&&source.release_ready&&!samePerson(actor,source.created_by)&&!samePerson(actor,attempt.actor_id)&&(await nativeSalesReadFirst<{installed:boolean}>(SALES_RD_NOTIFICATION_RECONCILIATION_INSTALLED,[],database))?.installed)try{await requireQueuedCommandAuthority(database,actor,'commerce.approve');await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'sales.orders',purpose:'settle-existing'});canReview=true;}catch(error){if(!(error instanceof ApiError)||![403,409,402].includes(error.status))throw error;}
 return {attemptId:attempt.id,hasProviderIdentity,canReview};
}
