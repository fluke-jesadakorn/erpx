import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { HandoffRow } from '@erpx/server-projects/public/project-structure';
const projectMember = (alias: string) => `(iam_same_person(${alias}.owner_user_id,?,${alias}.company_id) OR EXISTS(SELECT 1 FROM staff_project_members member WHERE member.project_id=${alias}.id AND iam_same_person(member.user_id,?,${alias}.company_id)))`;
export function projectStructureSources(db: SqlDatabase) {
    return {
        /** Graph actuals are owned by the mounted native Projects graph read;
         * there is no TypeScript SQL reader. Kept only to satisfy the shared
         * repository port used by the retained handoff reads. */
        async graphActuals() { throw new Error('PROJECT_GRAPH_NATIVE_ONLY'); },
        async handoffs(companyId: string, id: string) { return db.prepare('SELECT * FROM staff_project_handoffs WHERE company_id=? AND project_id=? ORDER BY prepared_at DESC,id').bind(companyId, id).all<HandoffRow>(); },
        async handoffProject(id: string, companyId: string) { return db.prepare('SELECT project_id FROM staff_project_handoffs WHERE id=? AND company_id=?').bind(id, companyId).first<{
            project_id: string;
        }>(); },
        async handoffWork(companyId: string, userId: string, userId2: string, userId3: string, userId4: string) {
            return db.prepare(`SELECT h.id,h.version,h.project_id,h.recipient_id,h.prepared_by,h.status,h.prepared_at,h.decided_at,p.title,p.lifecycle_status FROM staff_project_handoffs h JOIN staff_projects p ON p.id=h.project_id AND p.company_id=h.company_id
  WHERE h.company_id=? AND (iam_same_person(h.recipient_id,?,h.company_id) OR iam_same_person(h.prepared_by,?,h.company_id)) AND ${projectMember('p')} ORDER BY COALESCE(h.decided_at,h.prepared_at) DESC,h.id LIMIT 200`).bind(companyId, userId, userId2, userId3, userId4).all<{
                id: string;
                version: number;
                project_id: string;
                recipient_id: string;
                prepared_by: string;
                status: string;
                prepared_at: number;
                decided_at: number | null;
                title: string;
                lifecycle_status: string;
            }>();
        } };
}
