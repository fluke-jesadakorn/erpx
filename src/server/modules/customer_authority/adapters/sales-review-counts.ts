import { finalRootOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/final-root-owner-projection-sources";
import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {WorkContext} from '../../identity_access/public/authorization';
function orderScope(actor: WorkContext, alias = "o") {
  return { sql: `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, args: [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId] };
}
export async function salesCustomerReviewCounts(write:SqlDatabase,actor:WorkContext){const s=orderScope(actor);return (await write.prepare(`SELECT 'identity' kind,COUNT(*) count FROM customer_identity_changes WHERE company_id=? AND status='pending' UNION ALL SELECT 'duplicates',COUNT(*) FROM customer_duplicate_reviews WHERE company_id=? AND status='pending' UNION ALL SELECT 'access',COUNT(*) FROM customer_access_bindings b JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id WHERE ${s.sql} AND b.status='pending'`).bind(actor.companyId, actor.companyId, ...s.args).all<{ kind: string; count: number }>()).results;}
