import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { projectAuthorityProbeSql } from './iam-probe-sql';
export function projectStructureSources(db: SqlDatabase) {
    return { async currentProjectAuthority(assignmentId: string, companyId: string, userId: string, role: string, now: number, now2: number) { return db.prepare(projectAuthorityProbeSql(" AND iam_role_allows(role,company_id,'records.read')")).bind(assignmentId, companyId, userId, role, now, now2).first(); },
        async currentWorkAuthority(assignmentId: string, companyId: string, userId: string, role: string, now: number, now2: number) { return db.prepare(projectAuthorityProbeSql("")).bind(assignmentId, companyId, userId, role, now, now2).first(); } };
}
