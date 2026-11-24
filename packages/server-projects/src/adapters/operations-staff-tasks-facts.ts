import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {projectMemberScopeSql,projectMemberScopeGuard} from "@erpx/server-projects/adapters/project-read-projections";
export function StaffTasksProjectsFacts(database: SqlDatabase) {
    return { guard13(key: string, value: "task-project", projectId: string, companyId: string, userId: string, userId2: string) {
            return assertion(database, key, value, projectMemberScopeGuard(projectMemberScopeSql), [projectId, companyId, userId, userId2]);
        } };
}
