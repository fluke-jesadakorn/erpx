import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {nodeActiveScope} from './organization-column-alias-families';
export async function legalPeopleActiveUnit(db: SqlDatabase, companyId: string, id: string, kind: string) { return Boolean(await db.prepare(nodeActiveScope('kind')).bind(id, companyId, kind).first()); }
export async function legalPeopleDepartmentInBranch(db: SqlDatabase, companyId: string, department: string, branch: string) { return Boolean(await db.prepare(nodeActiveScope('branch_id')).bind(department, companyId, branch).first()); }
