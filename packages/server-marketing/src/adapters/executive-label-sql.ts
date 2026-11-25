import "server-only";
import { commercialOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/commercial-owner-projection-sources";
import { marketingSelectHeadSql } from './select-head-sql';

/** Owner-local fixed label projection shared by both executive breakdowns.
 * The branch and department labels share one byte-identical query head; a
 * caller picks a closed variant and never supplies raw query text. Each
 * variant emits its original statement byte for byte, so no duplicate label
 * literal survives in the owner. r51 folds that terminal head into the
 * canonical owner-local renderer; the COALESCE wrapper and fallback stay at
 * this call site. */
export function marketingExecutiveLabelSql(variant: 'branch' | 'department'): string {
  const ref = variant === 'branch' ? 'c.branch_id' : 'c.department_id';
  const fallback = variant === 'branch' ? 'Branch' : 'Unassigned';
  const head = `n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=${ref} AND n.company_id=c.company_id`;
  return `COALESCE((${marketingSelectHeadSql(head)}),'${fallback}')`;
}
