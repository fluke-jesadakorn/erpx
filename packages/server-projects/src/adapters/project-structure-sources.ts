import { projectReadProjection as projectfinanceProjection } from '@erpx/server-finance/public/project-read-projections';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { Actual } from '@erpx/server-projects/public/project-structure';
import type { HandoffRow } from '@erpx/server-projects/public/project-structure';
const projectMember = (alias: string) => `(iam_same_person(${alias}.owner_user_id,?,${alias}.company_id) OR EXISTS(SELECT 1 FROM staff_project_members member WHERE member.project_id=${alias}.id AND iam_same_person(member.user_id,?,${alias}.company_id)))`;
export function projectStructureSources(db: SqlDatabase) {
    return { async graphActuals(projectId: string, companyId: string, projectId2: string, companyId2: string) {
            return db.prepare(`SELECT a.*,l.user_id,l.version current_source_version,l.state source_state,NULL::text journal_status,NULL::text journal_id
  FROM staff_project_package_actuals a JOIN staff_project_work_logs l ON a.source_kind='work_log' AND l.id=a.source_id WHERE a.project_id=? AND a.company_id=?
  UNION ALL SELECT a.*,NULL,c.version,c.state,j.status,j.id FROM staff_project_package_actuals a JOIN staff_project_cost_allocations c ON a.source_kind='cost_allocation' AND c.id=a.source_id
  ${projectfinanceProjection.actualLine} ${projectfinanceProjection.actualJournal} WHERE a.project_id=? AND a.company_id=? ORDER BY id`).bind(projectId, companyId, projectId2, companyId2).all<Actual>();
        },
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
