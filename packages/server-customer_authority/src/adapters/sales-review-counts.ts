import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { CA_RD_SALES_REVIEW_COUNTS, nativeOwnerReadAll } from './native-owner-read';

/**
 * Native owner read. The pending identity-change, duplicate-review and access
 * counts are planned by the Customer Authority owner registry and executed on
 * the caller's already-open unit. No TypeScript SQL remains for this family.
 */
export async function salesCustomerReviewCounts(write: SqlDatabase, actor: WorkContext) {
  const values = [
    actor.companyId,
    actor.companyId,
    actor.companyId,
    actor.branchId,
    actor.branchId,
    actor.departmentId,
    actor.departmentId,
  ];
  return (await nativeOwnerReadAll<{ kind: string; count: number }>(
    CA_RD_SALES_REVIEW_COUNTS,
    values,
    write,
    actor.holdingSlug,
  )).results;
}
