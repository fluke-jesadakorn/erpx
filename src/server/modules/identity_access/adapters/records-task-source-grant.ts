import type { WorkContext } from '../public/authorization';
import type { SqlDatabase } from '../../runtime/public/runtime/types';
export async function recordTaskSourceGrant(database: SqlDatabase, a: WorkContext, now: number) { return !!await database.prepare("SELECT 1 FROM assignments WHERE id=? AND company_id=? AND user_id=? AND role=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?) AND iam_role_allows(role,company_id,'records.read')").bind(a.assignmentId, a.companyId, a.userId, a.role, now, now).first(); }
