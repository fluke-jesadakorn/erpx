import { supportTicketSourceReadPredicate } from "@erpx/server-support/adapters/ticket-source-read-policy";
import { finalRootOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/final-root-owner-projection-sources";
import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {Row} from '@erpx/server-commerce/public/commerce';
function orderScope(actor: WorkContext, alias = "o") {
  return { sql: `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, args: [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId] };
}
import {supportTicketReadPredicate, supportWorkspacePurchaseAccess} from '@erpx/server-support/public/support';
export async function salesSupportCounts(write:SqlDatabase,actor:WorkContext,ids:string[]){if (!(await supportWorkspacePurchaseAccess(actor, write)).history) return [];const t=orderScope(actor,'t'),s=orderScope(actor),policy=supportTicketReadPredicate(actor),placeholders=ids.map(()=>'?').join(',');return (await write.prepare(`SELECT t.customer_id,COUNT(*) count FROM support_tickets t JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=t.order_id AND o.company_id=t.company_id AND o.party_id=t.customer_id WHERE ${t.sql} AND ${s.sql} AND ${policy.sql} AND t.customer_id IN (${placeholders}) AND t.status NOT IN ('closed','cancelled') GROUP BY t.customer_id`).bind(...t.args, ...s.args, ...policy.bindings, ...ids).all<{ customer_id: string; count: number }>()).results;}
export async function salesOpenSupportCases(write:SqlDatabase,actor:WorkContext,id:string){if (!(await supportWorkspacePurchaseAccess(actor, write)).history) return [];const t=orderScope(actor,'t'),s=orderScope(actor),policy=supportTicketReadPredicate(actor);return (await write.prepare(`SELECT t.id,t.title,t.status,t.due_date FROM support_tickets t JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=t.order_id AND o.company_id=t.company_id AND o.party_id=t.customer_id WHERE ${t.sql} AND ${s.sql} AND ${policy.sql} AND t.customer_id=? AND t.status NOT IN ('closed','cancelled') ORDER BY t.updated_at DESC,t.id LIMIT 30`).bind(...t.args, ...s.args, ...policy.bindings, id).all<Row>()).results;}

/** CRM can reference accessible cases without bypassing Support's record or source policy. */
export async function supportCustomerSourceCases(db: SqlDatabase, actor: WorkContext, customerId: string) {
  if (!(await supportWorkspacePurchaseAccess(actor, db)).history) return [];
  const scope = orderScope(actor, "t"), policy = supportTicketReadPredicate(actor), source = await supportTicketSourceReadPredicate(actor);
  return (await db.prepare(`SELECT t.id,t.title,t.status,t.order_id FROM support_tickets t
    WHERE ${scope.sql} AND ${policy.sql} AND ${source.sql} AND t.customer_id=?
    ORDER BY t.updated_at DESC,t.id LIMIT 50`).bind(...scope.args, ...policy.bindings, ...source.bindings, customerId).all<Row>()).results;
}
