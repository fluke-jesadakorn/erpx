import "server-only";
import { commercialOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/commercial-owner-projection-sources";

/** Owner-local fixed label projection shared by both executive breakdowns.
 * The branch and department labels share one byte-identical query head; a
 * caller picks a closed variant and never supplies raw query text. Each
 * variant emits its original statement byte for byte, so no duplicate label
 * literal survives in the owner. */
export function marketingExecutiveLabelSql(variant: 'branch' | 'department'): string {
  const ref = variant === 'branch' ? 'c.branch_id' : 'c.department_id';
  const fallback = variant === 'branch' ? 'Branch' : 'Unassigned';
  return `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=${ref} AND n.company_id=c.company_id),'${fallback}')`;
}
