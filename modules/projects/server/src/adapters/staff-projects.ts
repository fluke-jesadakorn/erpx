import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { StaffActor } from '@erpx/operations-server/public/staff-policy';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import {requireProjectPurchasedRead} from '@erpx/projects-server/adapters/purchases';
import { createStaffProjectsApplication, type StaffProjectsRepository } from '@erpx/projects-server/application/staff-projects';
import {nativeProjectVisibleProject,nativeProjectVisibleProjects,type ProjectActualsActor} from './native-lane-27/client';
async function exposed<T>(action: () => Promise<T>) { try {
    return await action();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
/** The native owner executes the visible-row SQL and projects the fixed row
 * shape; this adapter carries only identities and keeps its public signature. */
const caller = (actor: unknown) => actor as ProjectActualsActor;
function application(actor: ProjectActualsActor) {
    const repository: StaffProjectsRepository<SqlStatement> = { async visibleProject(id: string, companyId: string, userId: string, _userId2: string) {
            return nativeProjectVisibleProject({ ...actor, companyId, userId }, id);
        },
        async visibleProjects(companyId: string, userId: string, _userId2: string) {
            return { results: await nativeProjectVisibleProjects({ ...actor, companyId, userId }) };
        },
        };
    return createStaffProjectsApplication({ repository });
}
export async function getProject(db: SqlDatabase, actor: StaffActor, id: string) { await requireProjectPurchasedRead(db,actor,id);return exposed(() => application(caller(actor)).getProject(actor, id)); }
export async function listProjects(db: SqlDatabase, actor: StaffActor) { await requireProjectPurchasedRead(db,actor);return exposed(() => application(caller(actor)).listProjects(actor)); }
