import {assignmentReadSql} from '@erpx/server-identity_access/public/record-source-authority';
import {marketingSourceRowGuardSql} from './attachment-owner-version-guard';
/** Canonical owner-local record-scope authority witness predicate (r44).
 * The owner-local witness shares the byte-identical EXISTS source-row head with
 * the admitted guard builder; callers supply source/authority aliases and the
 * two COALESCE placeholders keep each caller's bind order. The one remaining
 * statement token is the owner-owned scope tuple; no caller SQL crosses this
 * boundary. FOR SHARE locks, command_receipts settlement and native read ports
 * stay untouched. */
export const marketingRecordScopeAuthoritySql=(source:string,authority:string)=>marketingSourceRowGuardSql(
  `(SELECT ${source}.company_id,COALESCE(${source}.branch_id,?::text) branch_id,COALESCE(${source}.department_id,?::text) department_id) ${authority}`,
  assignmentReadSql(authority,'records.read'),
);
