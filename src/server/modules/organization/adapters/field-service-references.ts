import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";

export type ServiceReferenceScope = { companyId: string; branchId: string | null; departmentId: string | null };
export type ServiceLocationReference = { id: string; kind: "branch" | "headquarters" | "department"; display_name: string; branch_id: string };

/** Service's included Organization reference supplies no organization editor. */
export async function fieldServiceLocations(database: SqlDatabase, scope: ServiceReferenceScope): Promise<ServiceLocationReference[]> {
  return (await database.prepare(`SELECT n.id,n.kind,n.display_name,
    CASE WHEN n.kind='department' THEN n.branch_id ELSE n.id END branch_id
    FROM organization_nodes n WHERE n.company_id=? AND n.status='active' AND n.kind IN ('branch','headquarters','department')
    AND (n.kind<>'department' OR EXISTS(SELECT 1 FROM organization_nodes branch WHERE branch.id=n.branch_id
      AND branch.company_id=n.company_id AND branch.kind IN ('branch','headquarters') AND branch.status='active'))
    AND (?::text IS NULL OR CASE WHEN n.kind='department' THEN n.branch_id ELSE n.id END=?)
    AND (?::text IS NULL OR n.kind='department' AND n.id=? OR n.kind IN ('branch','headquarters') AND EXISTS(
      SELECT 1 FROM organization_nodes department WHERE department.id=? AND department.company_id=n.company_id
      AND department.kind='department' AND department.status='active' AND department.branch_id=n.id))
    ORDER BY n.kind,n.display_name,n.id`)
    .bind(scope.companyId,scope.branchId,scope.branchId,scope.departmentId,scope.departmentId,scope.departmentId).all<ServiceLocationReference>()).results;
}
