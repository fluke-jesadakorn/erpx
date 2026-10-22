import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export async function legalPeopleActiveUnit(db: SqlDatabase, companyId: string, id: string, kind: string) { return Boolean(await db.prepare("SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind=? AND status='active'").bind(id, companyId, kind).first()); }
export async function legalPeopleDepartmentInBranch(db: SqlDatabase, companyId: string, department: string, branch: string) { return Boolean(await db.prepare("SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND branch_id=? AND status='active'").bind(department, companyId, branch).first()); }
