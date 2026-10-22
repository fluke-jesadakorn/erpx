import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function projectStructureSources(db: SqlDatabase) {
    return { async currentProjectAuthority(assignmentId: string, companyId: string, userId: string, role: string, now: number, now2: number) { return db.prepare("SELECT 1 FROM assignments WHERE id=? AND company_id=? AND user_id=? AND role=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?) AND iam_role_allows(role,company_id,'records.read')").bind(assignmentId, companyId, userId, role, now, now2).first(); },
        async currentWorkAuthority(assignmentId: string, companyId: string, userId: string, role: string, now: number, now2: number) { return db.prepare('SELECT 1 FROM assignments WHERE id=? AND company_id=? AND user_id=? AND role=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?)').bind(assignmentId, companyId, userId, role, now, now2).first(); } };
}
