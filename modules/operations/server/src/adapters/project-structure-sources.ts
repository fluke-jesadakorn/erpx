import { projectReadProjection as projectprojectsProjection } from '@erpx/projects-server/public/project-read-projections';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import type { GraphTask } from '@erpx/projects-server/public/project-structure';
import {OPERATIONS_TASK_LIST_HEAD} from './read-tail-r53-sources';
export function projectStructureSources(db: SqlDatabase) {
    return { async graphTasks(projectId: string, companyId: string) {
            return db.prepare(`${OPERATIONS_TASK_LIST_HEAD}version,t.assignee_user_id,b.milestone_id,b.version binding_version
  FROM staff_tasks t ${projectprojectsProjection.taskBinding} WHERE t.project_id=? AND t.company_id=? ORDER BY t.id LIMIT 1001`).bind(projectId, companyId).all<GraphTask>();
        } };
}
