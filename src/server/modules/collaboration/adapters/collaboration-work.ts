import 'server-only';
import { createHash, randomUUID } from 'node:crypto';
import { ApiError } from '../../runtime/public/http';
import { holdingResources } from '../../runtime/public/runtime';
import { requirePermission } from '../../identity_access/public/iam-permissions';
import { readSourceRecord, sourceRecordGuards } from '../../records/public/source-record-policy';
import { executeCommand, replayCommand } from '../../workflow/public/commands';
import type { WorkContext } from '../../identity_access/public/authorization';
import type { SqlStatement } from '../../runtime/public/runtime/types';
import type { SqlDatabase } from '../../runtime/public/runtime/types';
import { assertion } from '../../workflow/public/commands';
import type { Row, Card } from '../../collaboration/public/collaboration-work';
import { collaborationCardEligibilitySql as eligible } from '../../identity_access/public/collaboration-read-projections';
import { collaborationReadProjection as peopleRead } from '../../people_payroll/public/collaboration-read-projections';
import { createCollaborationWorkApplication, type CollaborationWorkRepository } from '../application/collaboration-work';
import { BusinessRuleError } from '../../shared';
import { collaborationWorkSources as people } from '../../people_payroll/public/collaboration-work-sources';
async function inward<T>(action: () => Promise<T>) {
    try {
        return await action();
    }
    catch (error) {
        if (error instanceof ApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function exposed<T>(action: () => Promise<T>) {
    try {
        return await action();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
function application(db: SqlDatabase) {
    const repository: CollaborationWorkRepository<SqlStatement> = { async recordRelations(companyId: string) {
            return db
                .prepare("SELECT * FROM collaboration_record_relations WHERE company_id=? AND status='active' ORDER BY updated_at DESC LIMIT 100")
                .bind(companyId)
                .all<Row>();
        }, async actionCards(companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) {
            return db
                .prepare(`SELECT c.*,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible,${peopleRead.cardOwnerName} owner_name FROM collaboration_action_cards c WHERE c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?) ORDER BY c.updated_at DESC LIMIT 100`)
                .bind(companyId, branchId, branchId2, departmentId, departmentId2)
                .all<Card>();
        }, async actionHistory(id: string) {
            return db
                .prepare("SELECT * FROM collaboration_action_events WHERE card_id=? ORDER BY version")
                .bind(id)
                .all<Row>();
        }, prepareRelation(id: string, companyId: string, branchId: string | null, departmentId: string | null, type: "commerce_order" | "support_ticket" | "field_service_order" | "staff_task" | "pos_sale" | "pms_record" | "asset" | "maintenance_plan" | "maintenance_work", id2: string, version: number, type2: "commerce_order" | "support_ticket" | "field_service_order" | "staff_task" | "pos_sale" | "pms_record" | "asset" | "maintenance_plan" | "maintenance_work", id3: string, version2: number, relation: "related" | "duplicates" | "blocks" | "depends_on", reason: string, userId: string, now: number, now2: number) {
            return db
                .prepare("INSERT INTO collaboration_record_relations(id,company_id,branch_id,department_id,source_type,source_id,source_version,target_type,target_id,target_version,relation,reason,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id, companyId, branchId, departmentId, type, id2, version, type2, id3, version2, relation, reason, userId, now, now2);
        }, prepareRelationEvent(value: string, id: string, userId: string, reason: string, now: number) {
            return db
                .prepare("INSERT INTO collaboration_relation_events VALUES(?,?,1,'create',? ,?,'active',?)")
                .bind(value, id, userId, reason, now);
        }, async recordRelation(id: string, companyId: string) { return db.prepare("SELECT * FROM collaboration_record_relations WHERE id=? AND company_id=?").bind(id, companyId).first<Row>(); }, guardRelationVersion(key: string, value: "relation", id: string, companyId: string, expectedVersion: number) {
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM collaboration_record_relations WHERE id=? AND company_id=? AND version=? AND status='active')", [id, companyId, expectedVersion]);
        }, prepareRelationRemoval(userId: string, now: number, now2: number, id: string) {
            return db
                .prepare("UPDATE collaboration_record_relations SET status='removed',removed_by=?,removed_at=?,version=version+1,updated_at=? WHERE id=?")
                .bind(userId, now, now2, id);
        }, prepareRelationRemovalEvent(value: string, id: string, value2: number, userId: string, why: string, now: number) {
            return db
                .prepare("INSERT INTO collaboration_relation_events VALUES(?,?,?,'remove',?,?,'removed',?)")
                .bind(value, id, value2, userId, why, now);
        }, prepareCard(id: string, companyId: string, branchId: string | null, departmentId: string | null, type: "commerce_order" | "support_ticket" | "field_service_order" | "staff_task" | "pos_sale" | "pms_record" | "asset" | "maintenance_plan" | "maintenance_work", id2: string, version: number, title: string, ownerId: string, dueAt: number, userId: string, now: number, now2: number) {
            return db
                .prepare("INSERT INTO collaboration_action_cards(id,company_id,branch_id,department_id,source_type,source_id,source_version,title,owner_id,due_at,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id, companyId, branchId, departmentId, type, id2, version, title, ownerId, dueAt, userId, now, now2);
        }, guardCardOwner(key: string, value: "owner", id: string) {
            return assertion(db, key, value, `EXISTS(SELECT 1 FROM collaboration_action_cards c WHERE c.id=? AND ${eligible})`, [id]);
        }, prepareCardEvent(value: string, id: string, userId: string, reason: string, ownerId: string, dueAt: number, now: number) {
            return db
                .prepare("INSERT INTO collaboration_action_events VALUES(?,?,1,'create',?,?, 'open',?,?,?)")
                .bind(value, id, userId, reason, ownerId, dueAt, now);
        }, prepareCardNotice(value: string, id: string) {
            return db
                .prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,owner_id,'collaboration_action',id,version,'New cross-record action card: review the linked source',updated_at FROM collaboration_action_cards WHERE id=?")
                .bind(value, id);
        }, async actionCard(id: string, companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) {
            return db
                .prepare(`SELECT c.*,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible FROM collaboration_action_cards c WHERE c.id=? AND c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?)`)
                .bind(id, companyId, branchId, branchId2, departmentId, departmentId2)
                .first<Card>();
        }, guardCardVersion(key: string, value: "version", id: string, expectedVersion: number) {
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM collaboration_action_cards WHERE id=? AND version=? AND status='open')", [id, expectedVersion]);
        }, prepareCardTransition(owner: string, due: number, status: "completed" | "cancelled" | "open", evidence: string | null, now: number, id: string) {
            return db
                .prepare("UPDATE collaboration_action_cards SET owner_id=?,due_at=?,status=?,completion_evidence=?,version=version+1,updated_at=? WHERE id=?")
                .bind(owner, due, status, evidence, now, id);
        }, guardReassignedOwner(key: string, value: "owner", id: string) {
            return assertion(db, key, value, `EXISTS(SELECT 1 FROM collaboration_action_cards c WHERE c.id=? AND ${eligible})`, [id]);
        }, prepareCardTransitionEvent(value: string, id: string, value2: number, action: "complete" | "reassign" | "cancel", userId: string, reason: string, status: "completed" | "cancelled" | "open", owner: string, due: number, now: number) {
            return db
                .prepare("INSERT INTO collaboration_action_events VALUES(?,?,?,?,?,?,?,?,?,?)")
                .bind(value, id, value2, action, userId, reason, status, owner, due, now);
        }, prepareCardTransitionNotice(value: string, id: string) {
            return db
                .prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,owner_id,'collaboration_action',id,version,'Cross-record action card updated: review the next action',updated_at FROM collaboration_action_cards WHERE id=?")
                .bind(value, id);
        }, ...people(db) };
    return createCollaborationWorkApplication({ repository, commands: { replay: (...args) => inward(() => replayCommand(db, ...args)), commit: (...args) => inward(() => executeCommand(db, ...args)) }, clock: { now: () => Date.now() }, identifiers: { next: () => randomUUID() }, sha256: (value) => createHash('sha256').update(value, 'utf8').digest('hex'), requirePermission: (...args) => requirePermission(...args), readSourceRecord: (...args) => inward(() => readSourceRecord(...args)), sourceRecordGuards: (...args) => inward(() => sourceRecordGuards(db, ...args)) });
}
export async function collaborationWorkspace(a: WorkContext) { const { write: db } = await holdingResources(a.holdingId); return exposed(() => application(db).collaborationWorkspace(a)); }
export async function collaborationExport(a: WorkContext) { const { write: db } = await holdingResources(a.holdingId); return exposed(() => application(db).collaborationExport(a)); }
export async function collaborationActionDetail(a: WorkContext, id: string) { const { write: db } = await holdingResources(a.holdingId); return exposed(() => application(db).collaborationActionDetail(a, id)); }
export async function createRecordRelation(a: WorkContext, raw: unknown, key: string) { const { write: db } = await holdingResources(a.holdingId); return exposed(() => application(db).createRecordRelation(a, raw, key)); }
export async function removeRecordRelation(a: WorkContext, id: string, expectedVersion: number, why: string, key: string) { const { write: db } = await holdingResources(a.holdingId); return exposed(() => application(db).removeRecordRelation(a, id, expectedVersion, why, key)); }
export async function createActionCard(a: WorkContext, raw: unknown, key: string) { const { write: db } = await holdingResources(a.holdingId); return exposed(() => application(db).createActionCard(a, raw, key)); }
export async function changeActionCard(a: WorkContext, id: string, raw: unknown, key: string) { const { write: db } = await holdingResources(a.holdingId); return exposed(() => application(db).changeActionCard(a, id, raw, key)); }
export async function collaborationActionWork(a: WorkContext) { const { write: db } = await holdingResources(a.holdingId); return exposed(() => application(db).collaborationActionWork(a)); }
