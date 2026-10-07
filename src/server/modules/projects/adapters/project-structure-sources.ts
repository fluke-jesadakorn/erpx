import { projectReadProjection as projectfinanceProjection } from '../../finance/public/project-read-projections';
import { projectReadProjection as projectoperationsProjection } from '../../operations/public/project-read-projections';
import type { SqlDatabase } from '../../runtime/public/runtime/types';
import { assertion } from '../../workflow/public/commands';
import type { Actual } from '../../projects/public/project-structure';
import type { HandoffRow } from '../../projects/public/project-structure';
const projectMember = (alias: string) => `(iam_same_person(${alias}.owner_user_id,?,${alias}.company_id) OR EXISTS(SELECT 1 FROM staff_project_members member WHERE member.project_id=${alias}.id AND iam_same_person(member.user_id,?,${alias}.company_id)))`;
export function projectStructureSources(db: SqlDatabase) {
    return { async graphActuals(projectId: string, companyId: string, projectId2: string, companyId2: string) {
            return db.prepare(`SELECT a.*,l.user_id,l.version current_source_version,l.state source_state,NULL::text journal_status,NULL::text journal_id
  FROM staff_project_package_actuals a JOIN staff_project_work_logs l ON a.source_kind='work_log' AND l.id=a.source_id WHERE a.project_id=? AND a.company_id=?
  UNION ALL SELECT a.*,NULL,c.version,c.state,j.status,j.id FROM staff_project_package_actuals a JOIN staff_project_cost_allocations c ON a.source_kind='cost_allocation' AND c.id=a.source_id
  ${projectfinanceProjection.actualLine} ${projectfinanceProjection.actualJournal} WHERE a.project_id=? AND a.company_id=? ORDER BY id`).bind(projectId, companyId, projectId2, companyId2).all<Actual>();
        },
        preparePackageEvent(value: string, company_id: string, id: string, packageId: string | null, action: string, userId: string, note: string, value2: string, now: number) { return db.prepare('INSERT INTO staff_project_package_events VALUES(?,?,?,?,?,?,?,?,?)').bind(value, company_id, id, packageId, action, userId, note, value2, now); },
        async lockProject(projectId: string, companyId: string) { return db.prepare('SELECT id FROM staff_projects WHERE id=? AND company_id=? FOR UPDATE').bind(projectId, companyId).run(); },
        guardStructureMember(key: string, value: "project-member", id: string, companyId: string, expectedVersion: number, userId: string, userId2: string) { return assertion(db, key, value, `EXISTS(SELECT 1 FROM staff_projects p WHERE p.id=? AND p.company_id=? AND p.version=? AND ${projectMember('p')})`, [id, companyId, expectedVersion, userId, userId2]); },
        async taskBinding(taskId: string) { return db.prepare('SELECT * FROM staff_project_task_packages WHERE task_id=?').bind(taskId).first<{
            milestone_id: string;
            version: number;
        }>(); },
        guardAssignablePackage(key: string, value: "package-version", milestoneId: string, companyId: string, id: string, milestoneVersion: number) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM staff_project_milestones WHERE id=? AND company_id=? AND project_id=? AND version=? AND state IN ('planned','in_progress'))", [milestoneId, companyId, id, milestoneVersion]); },
        prepareTaskBinding(taskId: string, companyId: string, id: string, milestoneId: string, taskVersion: number, value: 1, userId: string, now: number) { return db.prepare('INSERT INTO staff_project_task_packages VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(task_id) DO UPDATE SET milestone_id=excluded.milestone_id,source_task_version=excluded.source_task_version,version=staff_project_task_packages.version+1,linked_by=excluded.linked_by,linked_at=excluded.linked_at').bind(taskId, companyId, id, milestoneId, taskVersion, value, userId, now); },
        guardUnassignedPackage(key: string, value: "package-editable", packageId: string | null) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM staff_project_milestones WHERE id=? AND state IN ('planned','in_progress'))", [packageId]); },
        prepareTaskUnbinding(taskId: string, bindingVersion: number) { return db.prepare('DELETE FROM staff_project_task_packages WHERE task_id=? AND version=?').bind(taskId, bindingVersion); },
        async approvedTime(sourceId: string, companyId: string, id: string, sourceVersion: number) { return db.prepare("SELECT * FROM staff_project_work_logs WHERE id=? AND company_id=? AND project_id=? AND state='approved' AND version=? FOR SHARE").bind(sourceId, companyId, id, sourceVersion).first<{
            minutes: number;
        }>(); },
        async approvedCost(sourceId: string, companyId: string, id: string, sourceVersion: number) { return db.prepare(`SELECT c.amount_minor,j.currency FROM staff_project_cost_allocations c ${projectfinanceProjection.costLine} ${projectfinanceProjection.expenseJournal} WHERE c.id=? AND c.company_id=? AND c.project_id=? AND c.state='approved' AND c.version=? AND j.status='posted' FOR SHARE OF c,j`).bind(sourceId, companyId, id, sourceVersion).first<{
            amount_minor: number;
            currency: string;
        }>(); },
        guardActualPackage(key: string, value: "actual-package-version", packageId: string, companyId: string, id: string, milestoneVersion: number) { return assertion(db, key, value, 'EXISTS(SELECT 1 FROM staff_project_milestones WHERE id=? AND company_id=? AND project_id=? AND version=?)', [packageId, companyId, id, milestoneVersion]); },
        prepareActualBinding(value: string, companyId: string, id: string, packageId: string, sourceKind: "work_log" | "cost_allocation", sourceId: string, sourceVersion: number, minutes: number | null, amount: number | null, currency: string | null, userId: string, now: number) { return db.prepare('INSERT INTO staff_project_package_actuals VALUES(?,?,?,?,?,?,?,?,?,?,?,?)').bind(value, companyId, id, packageId, sourceKind, sourceId, sourceVersion, minutes, amount, currency, userId, now); },
        async handoffReadiness(id: string, id2: string, id3: string, id4: string, id5: string, id6: string, id7: string) {
            return db.prepare(`SELECT NOT EXISTS(SELECT 1 FROM staff_project_milestones WHERE project_id=? AND state NOT IN ('completed','cancelled'))
   AND ${projectoperationsProjection.unfinishedTasks}
   AND ${projectoperationsProjection.unassignedTasks}
   AND NOT EXISTS(SELECT 1 FROM staff_project_work_logs WHERE project_id=? AND state='submitted')
   AND NOT EXISTS(SELECT 1 FROM staff_project_cost_allocations WHERE project_id=? AND state='draft')
   AND NOT EXISTS(SELECT 1 FROM staff_project_work_logs l WHERE l.project_id=? AND l.state='approved' AND NOT EXISTS(SELECT 1 FROM staff_project_package_actuals a WHERE a.source_kind='work_log' AND a.source_id=l.id))
   AND NOT EXISTS(SELECT 1 FROM staff_project_cost_allocations c ${projectfinanceProjection.actualLine} ${projectfinanceProjection.actualJournal}
    WHERE c.project_id=? AND c.state='approved' AND j.status='posted' AND NOT EXISTS(SELECT 1 FROM staff_project_package_actuals a WHERE a.source_kind='cost_allocation' AND a.source_id=c.id)) ready`).bind(id, id2, id3, id4, id5, id6, id7).first<{
                ready: boolean;
            }>();
        },
        guardRecipientMember(key: string, value: "recipient-member", id: string, recipientId: string) { return assertion(db, key, value, 'EXISTS(SELECT 1 FROM staff_project_members WHERE project_id=? AND user_id=?)', [id, recipientId]); },
        prepareHandoff(handoffId: string, companyId: string, id: string, version: number, recipientId: string, snapshot: string, digest: string, evidence: string, userId: string, now: number) { return db.prepare('INSERT INTO staff_project_handoffs(id,company_id,project_id,source_project_version,recipient_id,snapshot_json,snapshot_sha256,evidence,prepared_by,prepared_at) VALUES(?,?,?,?,?,?,?,?,?,?)').bind(handoffId, companyId, id, version, recipientId, snapshot, digest, evidence, userId, now); },
        async currentHandoff(handoffId: string, companyId: string, id: string) { return db.prepare('SELECT * FROM staff_project_handoffs WHERE id=? AND company_id=? AND project_id=? FOR UPDATE').bind(handoffId, companyId, id).first<{
            id: string;
            status: string;
            version: number;
            recipient_id: string;
            prepared_by: string;
            snapshot_sha256: string;
        }>(); },
        prepareHandoffDecision(status: "cancelled" | "accepted" | "rejected", userId: string, now: number, reason: string, id: string) { return db.prepare('UPDATE staff_project_handoffs SET status=?,version=version+1,decided_by=?,decided_at=?,decision_reason=? WHERE id=?').bind(status, userId, now, reason, id); },
        prepareLifecycle(lifecycle: "active" | "handoff_pending" | "completed" | "cancelled", now: number, id: string) { return db.prepare('UPDATE staff_projects SET lifecycle_status=?,version=version+1,updated_at=? WHERE id=?').bind(lifecycle, now, id); },
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
