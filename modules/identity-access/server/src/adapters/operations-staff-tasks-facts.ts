import {identitySnapshotSources as operationsSnapshotSources} from '@erpx/operations-server/public/identity-snapshot-sources';
const operationsSources=operationsSnapshotSources();
import {projectReadProjection} from "@erpx/projects-server/public/project-read-projections";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
export function StaffTasksIdentityAccessFacts(database: SqlDatabase) {
    return { guard11(key: string, value: "task-configure", id: string, companyId: string, userId: string, userId2: string) {
            return assertion(database, key, value, `EXISTS(SELECT 1 ${operationsSources.operations_staff_tasks_facts_staff_tasks_t_from} ${projectReadProjection.taskProject} WHERE t.id=? AND t.company_id=? AND (iam_same_person(t.creator_user_id,?,t.company_id) OR iam_same_person(p.owner_user_id,?,t.company_id)))`, [id, companyId, userId, userId2]);
        } };
}
