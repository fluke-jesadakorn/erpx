import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import { BusinessRuleError } from '@erpx/server-shared/index';
import type { ProjectMilestoneRow } from '@erpx/server-projects/domain/milestones';
import { createProjectMilestoneApplication } from '@erpx/server-projects/application/milestones';
import { ownedProjectWbs } from '@erpx/feature-projects/public/native-project-wbs-owned';

/** The constraint mapper is a pure application fact; no repository is reached. */
export function projectConstraint(error: unknown): never {
    try {
        return createProjectMilestoneApplication({
            repository: { async milestoneRows() { throw new Error('PROJECT_CONSTRAINT_ONLY'); } },
            getProject: async () => { throw new Error('PROJECT_CONSTRAINT_ONLY'); },
        }).projectConstraint(error);
    }
    catch (failure) {
        if (failure instanceof BusinessRuleError) {
            const out = new ApiError(failure.status, failure.code, failure.message);
            out.cause = failure.cause;
            throw out;
        }
        throw failure;
    }
}

/** Native Projects WBS milestones. The mounted owner read performs the
 * Company/membership, purchase, context and serializable-boundary checks; there
 * is no TypeScript query and no TS reader fallback. The retained `db` first
 * parameter keeps the shared Staff API port signature unchanged. */
export async function listProjectMilestones(
    _db: SqlDatabase,
    actor: WorkContext,
    projectId: string,
): Promise<ProjectMilestoneRow[]> {
    const wbs = await ownedProjectWbs(actor, actor.holdingSlug, projectId);
    return wbs?.milestones ?? [];
}

/** Explicit native-name export for consumers switching off the TS reader path. */
export const nativeProjectMilestones = listProjectMilestones;
