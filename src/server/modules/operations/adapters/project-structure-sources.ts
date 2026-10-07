import { projectReadProjection as projectprojectsProjection } from '../../projects/public/project-read-projections';
import type { SqlDatabase } from '../../runtime/public/runtime/types';
import type { GraphTask } from '../../projects/public/project-structure';
export function projectStructureSources(db: SqlDatabase) {
    return { async graphTasks(projectId: string, companyId: string) {
            return db.prepare(`SELECT t.id,t.title,t.state,t.version,t.assignee_user_id,b.milestone_id,b.version binding_version
  FROM staff_tasks t ${projectprojectsProjection.taskBinding} WHERE t.project_id=? AND t.company_id=? ORDER BY t.id LIMIT 1001`).bind(projectId, companyId).all<GraphTask>();
        },
        async currentTask(taskId: string, companyId: string, id: string) { return db.prepare('SELECT id,version,project_id FROM staff_tasks WHERE id=? AND company_id=? AND project_id=? FOR UPDATE').bind(taskId, companyId, id).first<{
            id: string;
            version: number;
            project_id: string;
        }>(); } };
}
