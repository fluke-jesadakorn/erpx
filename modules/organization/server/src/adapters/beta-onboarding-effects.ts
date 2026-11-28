import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeOrgStatement } from './native-organization-owner-write';
/** Retained team branch/department topology effects only. Company, staff-level
 * and CEO-position seeding belongs exclusively to the native onboarding
 * command. Effects are authoritative owner statements executed in the caller's
 * transaction; no TypeScript SQL copy remains. */
export function betaOnboardingEffects(db: SqlDatabase, companyId = '') {
    return {
        prepareBranch(id: string, slug: string, name: string, company: string, company2: string, id2: string): Promise<SqlStatement> {
            return nativeOrgStatement(db, companyId, 'beta_branch_insert', [id, slug, name, company, company2, id2], `beta-branch:${id}`, Date.now());
        },
        prepareDepartment(value: string, id: string, company: string, id2: string): Promise<SqlStatement> {
            return nativeOrgStatement(db, companyId, 'beta_department_insert', [value, id, company, id2], `beta-department:${value}`, Date.now());
        },
    };
}
