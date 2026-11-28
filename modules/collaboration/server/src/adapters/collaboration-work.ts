import 'server-only';
import {hasModuleAccess} from '@erpx/platform-catalogue/module-access';
import { createHash, randomUUID } from 'node:crypto';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import { nativeCardHistoryStatement, nativeCollaborationReadStatement } from './native-card-history-statement';
import { holdingResources } from '../../../../../platform/runtime/server/src/public/runtime';
import { requirePermission } from '@erpx/identity-access-server/public/iam-permissions';
import { readSourceRecord, sourceRecordGuards } from '@erpx/records-server/public/source-record-policy';
import { executeCommand, replayCommand } from '@erpx/workflow-server/public/commands';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import type { SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import type { Row, Card } from '@erpx/collaboration-server/public/collaboration-work';
import { createCollaborationWorkApplication, type CollaborationWorkRepository } from '@erpx/collaboration-server/application/collaboration-work';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import { collaborationWorkSources as people } from '@erpx/people-payroll-server/public/collaboration-work-sources';
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
    const repository: CollaborationWorkRepository = { async recordRelations(actor: WorkContext) {
            return nativeCollaborationReadStatement(db, actor, 'collaboration_record_relations', 'relations', randomUUID(), Date.now());
        }, async actionCards(actor: WorkContext) {
            const read = await nativeCollaborationReadStatement(db, actor, 'collaboration_action_cards', 'cards', randomUUID(), Date.now());
            return { results: read.results as Card[] };
        }, async actionHistory(actor: WorkContext, id: string) {
            return nativeCardHistoryStatement(db, actor, id, randomUUID(), Date.now());
        }, ...people(db) };
    return createCollaborationWorkApplication<SqlStatement>({ repository, commands: { replay: (...args) => inward(() => replayCommand(db, ...args)), commit: (...args) => inward(() => executeCommand(db, ...args)) }, clock: { now: () => Date.now() }, identifiers: { next: () => randomUUID() }, sha256: (value) => createHash('sha256').update(value, 'utf8').digest('hex'), requirePermission: (...args) => requirePermission(...args), readSourceRecord: (...args) => inward(() => readSourceRecord(...args)), sourceRecordGuards: (...args) => inward(() => sourceRecordGuards(db, ...args)) });
}
export async function collaborationWorkspace(a: WorkContext) { const { write: db } = await holdingResources(a.holdingId); return exposed(() => application(db).collaborationWorkspace(a)); }
export async function collaborationExport(a: WorkContext) { const { write: db } = await holdingResources(a.holdingId); return exposed(() => application(db).collaborationExport(a)); }
export async function collaborationActionDetail(a: WorkContext, id: string) { const { write: db } = await holdingResources(a.holdingId); return exposed(() => application(db).collaborationActionDetail(a, id)); }
export async function collaborationActionWork(a:WorkContext,nativeField?:()=>Promise<import('@erpx/work-projection-ui/lib/work-item').WorkItem[]>){
 if(!hasModuleAccess(a.modules,'collaboration'))return [];
 const field=hasModuleAccess(a.modules,'field_service');
 if(field&&!nativeField)throw new ApiError(503,'NATIVE_FIELD_COLLABORATION_WORK_REQUIRED','The native Field Collaboration source must be connected.');
 const {write:db}=await holdingResources(a.holdingId);
 const [legacy,native]=await Promise.all([exposed(()=>application(db).collaborationActionWork(a)),field?nativeField!():Promise.resolve([])]);
 return [...legacy,...native];
}
