import 'server-only';
import {createHash} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {checkPurchasedCapability,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import type {CapabilityPurpose} from '@erpx/server-commercial/public/packages';
import {commerceCompleteOwnerSourceReadLocks} from '@erpx/server-commerce/public/owner-source-locks';
import {attachmentPrivateReadSourceLocks} from '@erpx/server-files/public/private-attachment-reads';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {supportTicketPortalInstalled,supportTicketPortalSourceLocks} from '@erpx/server-support/adapters/ticket-portal-sources';
export const ticketPortalDigest=(value:unknown)=>createHash('sha256').update(JSON.stringify(value)).digest('hex');
export const ticketPortalChanged=()=>new ApiError(409,'SUPPORT_PORTAL_SOURCE_CHANGED','The original ticket, verified customer access or public response changed before completion.');
export function ticketPortalKey(key:string){if(!/^[A-Za-z0-9:_.-]{8,180}$/.test(key))throw new ApiError(422,'COMMAND_KEY_REQUIRED','Use a valid Idempotency-Key.');}
export async function ticketPortalCapability(database:SqlDatabase,companyId:string,purpose:CapabilityPurpose){for(const capability of ['support.tickets','support.portal'])await checkPurchasedCapability(database,'enforced',{companyId,capability,purpose});if(!await database.prepare(`SELECT ${scheduledFeaturePredicate('portal_scope.company_id','support')} allowed FROM (SELECT ?::text company_id) portal_scope`).bind(companyId).first<boolean>('allowed'))throw new ApiError(403,'FEATURE_COMING_SOON','Support is not released for this Company.');}
export async function ticketPortalStaffAuthority(database:SqlDatabase,actor:WorkContext,manage=false){for(const permission of ['support.read',...(manage?['support.manage']:[])]){requirePermission(actor,permission);await requireQueuedCommandAuthority(database,actor,permission);}}
/** Package-first excludes enabled ordinary writers; receipt SRE precedes all
 * business leases for original disabled receipt-first Support/Files writers.
 * Every lease is query-free, before IAM/advisory or the first source snapshot. */
export async function withSupportTicketPortalUnit<Result>(owner:SqlDatabase,run:(database:SqlDatabase,final:(guard:()=>Promise<void>)=>void)=>Promise<Result>,staff?:WorkContext,assertCurrent?:()=>Promise<void>){
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'SUPPORT_PORTAL_TRANSACTION_REQUIRED','Use the complete Support Portal owner transaction.');
 if(!await supportTicketPortalInstalled(owner))throw new ApiError(503,'SUPPORT_PORTAL_NOT_INSTALLED','Support Portal storage is not installed.');
 try{return await owner.transaction(async database=>{
  await lockPackageSettlementSources(database);
  await database.prepare('LOCK TABLE command_receipts,command_assertions,audit_events,outbox_events IN SHARE ROW EXCLUSIVE MODE').run();
  await database.batch([...commerceCompleteOwnerSourceReadLocks(database),...supportTicketPortalSourceLocks(database),attachmentPrivateReadSourceLocks(database)]);
  if(staff)await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);
  if(staff)await database.prepare('SELECT pg_advisory_xact_lock(hashtextextended(?,0))').bind(`commercial.package:company:${staff.companyId}`).run();
  await assertCurrent?.();const guards:(()=>Promise<void>)[]=[],result=await run(database,guard=>guards.push(guard));
  // No owner writes follow this flush. All final source, purchase, native IAM
  // and Control session checks are read-only while the original leases remain.
  await flushFinanceDeferredEffects(database);for(const guard of guards)await guard();await assertCurrent?.();return result;
 });}catch(error){let cause=error;for(let depth=0;depth<6&&cause&&typeof cause==='object';depth++){const current=cause as {code?:string;message?:string;cause?:unknown};if(current.code==='23514'&&current.message?.includes('SUPPORT_PORTAL_SOURCE_CHANGED'))throw ticketPortalChanged();cause=current.cause;}throw purchaseCommandFailure(error)??error;}
}
