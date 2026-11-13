import 'server-only';
import {createHash} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {samePerson} from '@erpx/feature-identity_access/lib/iam';
import {requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {salesNotificationDeliverySources} from '@erpx/server-commerce/public/sales-notification-sources';
import {salesNotificationCustomerSources} from '@erpx/server-finance/public/sales-notification-sources';
import {commercePackageEnforcementMode,checkPurchasedCapability} from '@erpx/server-commercial/public/purchase-command-boundary';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import type {CustomerMail} from '@erpx/server-sales/domain/notifications';
/** Retained read-only notice projections. Settings/queue/retry are native-owned
 * commands; this module supplies the original source witness and the
 * presentation flag for the native reconciliation reviewer. */
export type SalesNotificationSource=CustomerMail&{version:number;status:string;created_by:string;provider_message_id:string|null;lease_until:number|null;next_attempt_at:number;branch_id:string|null;department_id:string|null;order_state:string|null;customer_status:string|null;notification_email:string|null;notification_consent:string|null;current_adapter:string|null;source_company:string|null;customer_company:string|null;release_ready:boolean;image:string};
const digest=(image:string)=>createHash('sha256').update(image).digest('hex');
export const salesNotificationSourceHash=(source:SalesNotificationSource)=>digest(source.image);
/** Actual same-Company fulfillment and independently managed customer, never a
 * supplied Sales label or a private snapshot sent to the browser. */
export function salesNotificationSource(database:SqlDatabase,id:string){return database.prepare(`SELECT n.*,delivery.department_id,delivery.order_state,delivery.company_id source_company,customer.company_id customer_company,customer.status customer_status,customer.notification_email,customer.notification_consent,settings.adapter current_adapter,${scheduledFeaturePredicate('n.company_id','sales')} release_ready,
 jsonb_build_array(n.id,n.company_id,n.branch_id,n.order_id,n.fulfillment_id,n.party_id,n.party_version,n.recipient_email,n.subject,n.body_text,n.adapter,n.created_by,n.created_at,delivery.fulfillment_id,delivery.order_id,delivery.company_id,delivery.branch_id,delivery.department_id,delivery.party_id,delivery.order_kind,customer.id,customer.company_id,customer.kind)::text image
 FROM commerce_customer_notifications n LEFT JOIN (${salesNotificationDeliverySources}) delivery ON delivery.fulfillment_id=n.fulfillment_id AND delivery.order_id=n.order_id AND delivery.company_id=n.company_id AND delivery.party_id=n.party_id
 LEFT JOIN (${salesNotificationCustomerSources}) customer ON customer.id=n.party_id AND customer.company_id=n.company_id
 LEFT JOIN commerce_customer_notification_settings settings ON settings.company_id=n.company_id WHERE n.id=?`).bind(id).first<SalesNotificationSource>();}
export async function requireSalesNotificationRead(database:SqlDatabase,actor:WorkContext){const mode=await commercePackageEnforcementMode();if(mode==='disabled')return;await requireQueuedCommandAuthority(database,actor,'commerce.read');await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'sales.orders',purpose:'history'});}

/** Minimal presentation flags after the original Sales record reader. No
 * provider response, private message or capability snapshot crosses the
 * server/client boundary. Reconciliation itself is native-owned; this retained
 * read only advertises whether the current reviewer may invoke it. */
export async function customerNotificationReconciliationControls(actor:WorkContext,id:string){
 const mode=await commercePackageEnforcementMode();if(mode==='disabled')return undefined;const database=(await holdingResources(actor.holdingId)).write;await requireQueuedCommandAuthority(database,actor,'commerce.read');await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'sales.orders',purpose:'history'});const source=await salesNotificationSource(database,id);
 if(!source||source.company_id!==actor.companyId||source.source_company!==actor.companyId||source.customer_company!==actor.companyId||actor.branchId&&actor.branchId!==source.branch_id||actor.departmentId&&actor.departmentId!==source.department_id)throw new ApiError(404,'CUSTOMER_NOTIFICATION_NOT_FOUND','The original customer notice is unavailable in this work scope.');
 const attempt=source.status==='unknown'?await database.prepare("SELECT attempt.id,attempt.source_hash,attempt.provider_message_id,authority.actor_id FROM sales_notification_execution_attempts attempt JOIN sales_notification_execution_authorizations authority ON authority.command_id=attempt.authorization_command_id AND authority.company_id=attempt.company_id AND authority.notification_id=attempt.notification_id WHERE attempt.notification_id=? AND attempt.company_id=? AND attempt.attempt=? AND attempt.state='unknown'").bind(id,actor.companyId,source.attempts).first<{id:string;source_hash:string;provider_message_id:string|null;actor_id:string}>():null;
 if(!attempt)return undefined;const hasProviderIdentity=source.adapter==='resend'&&!!attempt.provider_message_id&&attempt.provider_message_id===source.provider_message_id&&attempt.source_hash===salesNotificationSourceHash(source);let canReview=false;
 if(hasProviderIdentity&&source.release_ready&&!samePerson(actor,source.created_by)&&!samePerson(actor,attempt.actor_id)&&await database.prepare("SELECT to_regclass('sales_notification_reconciliations') IS NOT NULL installed").first<boolean>('installed'))try{await requireQueuedCommandAuthority(database,actor,'commerce.approve');await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'sales.orders',purpose:'settle-existing'});canReview=true;}catch(error){if(!(error instanceof ApiError)||![403,409,402].includes(error.status))throw error;}
 return {attemptId:attempt.id,hasProviderIdentity,canReview};
}
