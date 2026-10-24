import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";
import 'server-only';
import type { z } from 'zod';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { holdingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { ApiError } from '@erpx/server-runtime/public/http';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { requirePermission } from '@erpx/server-identity_access/public/iam-permissions';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { pmsSourceReadableSql, pmsSourcesSql } from '@erpx/server-pms/public/pms/source-policy';
import { readablePmsSource, pmsSourceLink } from '@erpx/server-pms/public/pms/source';
import {getTask,taskSourceReadGuard} from '@erpx/server-operations/public/staff-task-read';
import {readablePost,sourceGuard} from '@erpx/server-collaboration/public/social-source';
import{createNativeSupportTicketRead}from '@erpx/server-support/public/support-ticket-read';
// The owner read callbacks preserve recursive source references on the same DB.
const{getSupportTicket,supportTicketSourceGuards}=createNativeSupportTicketRead({read:readSourceRecord,visible:visibleSourceRecord,guards:sourceRecordGuards});
import { createSourceRecordApplication } from '@erpx/server-records/application/source-record-policy';
import { sourceRecordInput, type AuthorizedSourceRecord } from '@erpx/server-records/domain/source-record-policy';
export { sourceRecordKind, sourceRecordInput, type AuthorizedSourceRecord } from '@erpx/server-records/domain/source-record-policy';
import { ownedRecordSource as commerceSource } from '@erpx/server-commerce/public/records-source-facts';
import { ownedRecordSource as field_serviceSource } from '@erpx/server-field_service/public/records-source-facts';
import { ownedRecordSource as posSource } from '@erpx/server-pos/public/records-source-facts';
import { ownedRecordSource as assetsSource } from '@erpx/server-assets/public/records-source-facts';
import { ownedRecordSource as maintenanceSource } from '@erpx/server-maintenance/public/records-source-facts';
function application(provided?: SqlDatabase): ReturnType<typeof createSourceRecordApplication<SqlStatement>> {
    let database = provided!;
    return createSourceRecordApplication<SqlStatement>({ clock: { now: Date.now }, prepare: async (holding) => { database = provided ?? (await holdingResources(holding)).write; }, authorize: (a, permission) => fromNativeSync(() => requirePermission(a, permission)), taskGrant: async (a, now) => !!await database.prepare(`SELECT 1 FROM (${identity_accessProjectionSources.assignments}) assignments WHERE id=? AND company_id=? AND user_id=? AND role=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?) AND iam_role_allows(role,company_id,'records.read')`).bind(a.assignmentId, a.companyId, a.userId, a.role, now, now).first(), task: (a, id) => fromNative(() => getTask(database, a, id)), pms: (a, id) => fromNative(() => readablePmsSource(database, a, id)), pmsLink: pmsSourceLink, support: (a, id) => fromNative(() => getSupportTicket(a, id, database)), source: (a, kind, id) => {
            switch (kind) {
                case 'commerce_order': return commerceSource(database).read(a, kind, id);
                case 'field_service_order': return field_serviceSource(database).read(a, kind, id);
                case 'pos_sale': return posSource(database).read(a, kind, id);
                case 'asset': return assetsSource(database).read(a, kind, id);
                case 'maintenance_plan': return maintenanceSource(database).read(a, kind, id);
                case 'maintenance_work': return maintenanceSource(database).read(a, kind, id);
            }
        }, taskGuard: (a, id, version, key, step) => taskSourceReadGuard(database, a, id, version, key, step), postGuard: async (a, id, key, step) => sourceGuard(database, key, await fromNative(() => readablePost(database, a, id)), a.userId, step), pmsGuard: (a, key, source, step) => assertion(database, key, step, `EXISTS(SELECT 1 FROM (${pmsSourcesSql}) s WHERE s.id=? AND s.company_id=? AND s.version=? AND ${pmsSourceReadableSql("s", "?::text", "?::text")})`, [source.id, a.companyId, source.version, a.userId, a.assignmentId]), supportGuards: (a, key, source, step) => fromNative(() => supportTicketSourceGuards(database, a, key, source.id, source.version, step)), sourceGuards: (a, key, source, step) => {
            switch (source.kind) {
                case 'commerce_order': return commerceSource(database).guards(a, key, { ...source, kind: source.kind }, step);
                case 'field_service_order': return field_serviceSource(database).guards(a, key, { ...source, kind: source.kind }, step);
                case 'pos_sale': return posSource(database).guards(a, key, { ...source, kind: source.kind }, step);
                case 'asset': return assetsSource(database).guards(a, key, { ...source, kind: source.kind }, step);
                case 'maintenance_plan': return maintenanceSource(database).guards(a, key, { ...source, kind: source.kind }, step);
                case 'maintenance_work': return maintenanceSource(database).guards(a, key, { ...source, kind: source.kind }, step);
                default: throw new ApiError(422, 'SOURCE_RECORD_INVALID', 'Unsupported source record.');
            }
        } });
}
function fromNativeSync<Result>(run: () => Result) {
    try {
        return run();
    }
    catch (error) {
        if (error instanceof ApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function fromNative<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof ApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function invoke<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function readSourceRecord(a: WorkContext, raw: z.infer<typeof sourceRecordInput>, db?: SqlDatabase): Promise<AuthorizedSourceRecord> { return invoke(() => application(db).readSourceRecord(a, raw)); }
export async function visibleSourceRecord(a: WorkContext, input: z.infer<typeof sourceRecordInput>, db?: SqlDatabase): Promise<AuthorizedSourceRecord | null> { return invoke(() => application(db).visibleSourceRecord(a, input)); }
export async function sourceRecordGuards(db: SqlDatabase, a: WorkContext, key: string, source: AuthorizedSourceRecord, step = 'source-record'): Promise<SqlStatement[]> { return invoke(() => application(db).sourceRecordGuards(a, key, source, step)); }
