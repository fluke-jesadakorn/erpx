import 'server-only';
import {hasModuleAccess} from '@erpx/catalogue/module-access';
import { createHash, randomUUID } from 'node:crypto';
import { ApiError } from '@erpx/server-runtime/public/http';
import { holdingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { requirePermission } from '@erpx/server-identity_access/public/iam-permissions';
import { readSourceRecord, sourceRecordGuards } from '@erpx/server-records/public/source-record-policy';
import { executeCommand, replayCommand } from '@erpx/server-workflow/public/commands';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { Row, Card } from '@erpx/server-collaboration/public/collaboration-work';
import { collaborationCardEligibilitySql as eligible } from '@erpx/server-identity_access/public/collaboration-read-projections';
import { collaborationReadProjection as peopleRead } from '@erpx/server-people_payroll/public/collaboration-read-projections';
import { createCollaborationWorkApplication, type CollaborationWorkRepository } from '@erpx/server-collaboration/application/collaboration-work';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { collaborationWorkSources as people } from '@erpx/server-people_payroll/public/collaboration-work-sources';
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
    const repository: CollaborationWorkRepository = { async recordRelations(companyId: string) {
            return db
                .prepare("SELECT * FROM collaboration_record_relations WHERE company_id=? AND status='active' AND source_type<>'field_service_order' AND target_type<>'field_service_order' ORDER BY updated_at DESC LIMIT 100")
                .bind(companyId)
                .all<Row>();
        }, async actionCards(companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) {
            return db
                .prepare(`SELECT c.*,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible,${peopleRead.cardOwnerName} owner_name FROM collaboration_action_cards c WHERE c.company_id=? AND c.source_type<>'field_service_order' AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?) ORDER BY c.updated_at DESC LIMIT 100`)
                .bind(companyId, branchId, branchId2, departmentId, departmentId2)
                .all<Card>();
        }, async actionHistory(id: string) {
            return db
                .prepare("SELECT * FROM collaboration_action_events WHERE card_id=? ORDER BY version")
                .bind(id)
                .all<Row>();
        }, ...people(db) };
    return createCollaborationWorkApplication<SqlStatement>({ repository, commands: { replay: (...args) => inward(() => replayCommand(db, ...args)), commit: (...args) => inward(() => executeCommand(db, ...args)) }, clock: { now: () => Date.now() }, identifiers: { next: () => randomUUID() }, sha256: (value) => createHash('sha256').update(value, 'utf8').digest('hex'), requirePermission: (...args) => requirePermission(...args), readSourceRecord: (...args) => inward(() => readSourceRecord(...args)), sourceRecordGuards: (...args) => inward(() => sourceRecordGuards(db, ...args)) });
}
export async function collaborationWorkspace(a: WorkContext) { const { write: db } = await holdingResources(a.holdingId); return exposed(() => application(db).collaborationWorkspace(a)); }
export async function collaborationExport(a: WorkContext) { const { write: db } = await holdingResources(a.holdingId); return exposed(() => application(db).collaborationExport(a)); }
export async function collaborationActionDetail(a: WorkContext, id: string) { const { write: db } = await holdingResources(a.holdingId); return exposed(() => application(db).collaborationActionDetail(a, id)); }
export async function collaborationActionWork(a:WorkContext,nativeField?:()=>Promise<import('@erpx/feature-work_projection/lib/work-item').WorkItem[]>){
 if(!hasModuleAccess(a.modules,'collaboration'))return [];
 const field=hasModuleAccess(a.modules,'field_service');
 if(field&&!nativeField)throw new ApiError(503,'NATIVE_FIELD_COLLABORATION_WORK_REQUIRED','The native Field Collaboration source must be connected.');
 const {write:db}=await holdingResources(a.holdingId);
 const [legacy,native]=await Promise.all([exposed(()=>application(db).collaborationActionWork(a)),field?nativeField!():Promise.resolve([])]);
 return [...legacy,...native];
}
