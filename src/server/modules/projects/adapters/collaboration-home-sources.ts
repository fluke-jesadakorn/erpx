import type { SqlDatabase } from '../../runtime/public/runtime/types';
import { assertion } from '../../workflow/public/commands';
export function collaborationHomeSources(db: SqlDatabase) {
    return { async projects(companyId: string, userId: string, userId2: string) {
            return db.prepare(`SELECT p.id,p.title FROM staff_projects p WHERE p.company_id=?
      AND (p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?))
      ORDER BY p.title,p.id LIMIT 100`)
                .bind(companyId, userId, userId2).all<{
                id: string;
                title: string;
            }>();
        }, guardProjectAudience(key: string, value: "audience-project", id: string, companyId: string, userId: string, userId2: string) {
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM staff_projects p WHERE p.id=? AND p.company_id=? AND (p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?)))", [id, companyId, userId, userId2]);
        } };
}
