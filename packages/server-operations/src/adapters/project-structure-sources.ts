import { projectReadProjection as projectprojectsProjection } from '@erpx/server-projects/public/project-read-projections';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { GraphTask } from '@erpx/server-projects/public/project-structure';
export function projectStructureSources(db: SqlDatabase) {
    return { async graphTasks(projectId: string, companyId: string) {
            return db.prepare(`SELECT t.id,t.title,t.state,t.version,t.assignee_user_id,b.milestone_id,b.version binding_version
  FROM staff_tasks t ${projectprojectsProjection.taskBinding} WHERE t.project_id=? AND t.company_id=? ORDER BY t.id LIMIT 1001`).bind(projectId, companyId).all<GraphTask>();
        } };
}
