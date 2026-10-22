import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
export function StaffTasksProjectsFacts(database: SqlDatabase) {
    return { guard13(key: string, value: "task-project", projectId: string, companyId: string, userId: string, userId2: string) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM staff_projects p WHERE p.id=? AND p.company_id=? AND (p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?)))", [projectId, companyId, userId, userId2]);
        } };
}
