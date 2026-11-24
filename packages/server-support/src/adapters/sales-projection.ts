import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {Row} from '@erpx/server-commerce/public/commerce';
import {supportWorkspacePurchaseAccess} from '@erpx/server-support/public/support';
import {supportTicketSourceIdSets} from '@erpx/server-support/adapters/ticket-source-read-policy';
import {
  nativeSupportReadAll,
  SUPPORT_RD_SALES_SUPPORT_COUNTS,
  SUPPORT_RD_SALES_OPEN_SUPPORT_CASES,
  SUPPORT_RD_SALES_CUSTOMER_SOURCE_CASES,
} from '@erpx/server-support/adapters/native-support-read';

/** Order-scope and ticket-actor policy values stay scalar binds; the scope,
 * Commerce order projection and source predicates are fixed owner SQL in the
 * native read registry. No TypeScript SQL or customer content is emitted. */
const scopeBinds = (actor: WorkContext): unknown[] => [
  actor.companyId,
  actor.branchId,
  actor.branchId,
  actor.departmentId,
  actor.departmentId,
];
const policyBinds = (actor: WorkContext): unknown[] => [actor.assignmentId, actor.userId, actor.role];

export async function salesSupportCounts(write:SqlDatabase,actor:WorkContext,ids:string[]){
  if (!(await supportWorkspacePurchaseAccess(actor, write)).history) return [];
  return (await nativeSupportReadAll<{ customer_id: string; count: number }>(
    SUPPORT_RD_SALES_SUPPORT_COUNTS,
    [...scopeBinds(actor), ...scopeBinds(actor), ...policyBinds(actor), JSON.stringify(ids)],
    write,
  )).results;
}

export async function salesOpenSupportCases(write:SqlDatabase,actor:WorkContext,id:string){
  if (!(await supportWorkspacePurchaseAccess(actor, write)).history) return [];
  return (await nativeSupportReadAll<Row>(
    SUPPORT_RD_SALES_OPEN_SUPPORT_CASES,
    [...scopeBinds(actor), ...scopeBinds(actor), ...policyBinds(actor), id],
    write,
  )).results;
}

/** CRM can reference accessible cases without bypassing Support's record or source policy. */
export async function supportCustomerSourceCases(db: SqlDatabase, actor: WorkContext, customerId: string) {
  if (!(await supportWorkspacePurchaseAccess(actor, db)).history) return [];
  const source = await supportTicketSourceIdSets(actor, db);
  return (await nativeSupportReadAll<Row>(
    SUPPORT_RD_SALES_CUSTOMER_SOURCE_CASES,
    [...scopeBinds(actor), ...policyBinds(actor),
      JSON.stringify(source.posSale), JSON.stringify(source.asset), JSON.stringify(source.pmsRecord),
      customerId],
    db,
  )).results;
}
