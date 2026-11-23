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
import {nativeSourceRecord} from '@erpx/server-field_service/public/native-source-record';
import { ownedRecordSource as posSource } from '@erpx/server-pos/public/records-source-facts';
import { ownedRecordSource as assetsSource } from '@erpx/server-assets/public/records-source-facts';
import { headers } from 'next/headers';
import type { SourceRow } from '@erpx/server-records/public/source-record-contract';
/** Native Maintenance record_source bridge. The owning Rust port
 * (`erpx_server_maintenance::public::record_source`) reads the plan/work row
 * inside the caller's serializable unit; this transport carries only the scoped
 * read to that native owner. No owner SQL or fallback remains in TypeScript. */
type MaintenanceSourceKind = 'maintenance_plan' | 'maintenance_work';
type MaintenanceNativeSettings = {
    privateBaseUrl: string;
    timeoutMs: number;
    holdings: { origin: string; holdingSlug: string; holdingId: string }[];
};
const maintFail = (status: number, code: string): never => {
    throw new ApiError(status, code, 'The native Maintenance source record is unavailable.');
};
const maintObject = (value: unknown): value is Record<string, unknown> =>
    !!value && typeof value === 'object' && !Array.isArray(value);
const maintText = (value: unknown): value is string => typeof value === 'string' && value.length > 0;
function maintConfig(raw: string): MaintenanceNativeSettings {
    let value: unknown;
    try { value = JSON.parse(raw); } catch { return maintFail(503, 'NATIVE_MAINTENANCE_SOURCE_RECORD_CONFIG_INVALID'); }
    if (!maintObject(value) || typeof value.privateBaseUrl !== 'string' ||
        !Number.isSafeInteger(value.timeoutMs) || (value.timeoutMs as number) < 250 || (value.timeoutMs as number) > 30000 ||
        !Array.isArray(value.holdings) || value.holdings.length < 1 || value.holdings.length > 64)
        return maintFail(503, 'NATIVE_MAINTENANCE_SOURCE_RECORD_CONFIG_INVALID');
    let base: URL;
    try { base = new URL(value.privateBaseUrl); } catch { return maintFail(503, 'NATIVE_MAINTENANCE_SOURCE_RECORD_CONFIG_INVALID'); }
    if (base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port ||
        base.username || base.password || value.privateBaseUrl !== base.origin)
        return maintFail(503, 'NATIVE_MAINTENANCE_SOURCE_RECORD_CONFIG_INVALID');
    for (const holding of value.holdings) {
        if (!maintObject(holding) || typeof holding.origin !== 'string' ||
            typeof holding.holdingSlug !== 'string' || !maintText(holding.holdingId))
            return maintFail(503, 'NATIVE_MAINTENANCE_SOURCE_RECORD_CONFIG_INVALID');
        try {
            const origin = new URL(holding.origin);
            if (origin.protocol !== 'https:' || holding.origin !== origin.origin || origin.port)
                return maintFail(503, 'NATIVE_MAINTENANCE_SOURCE_RECORD_CONFIG_INVALID');
        } catch { return maintFail(503, 'NATIVE_MAINTENANCE_SOURCE_RECORD_CONFIG_INVALID'); }
    }
    return value as MaintenanceNativeSettings;
}
export async function nativeMaintenanceSourceRecord(actor: WorkContext, kind: MaintenanceSourceKind, id: string, version?: number): Promise<SourceRow | undefined> {
    const raw = process.env.ERPX_NATIVE_MAINTENANCE_CONFIG;
    if (raw === undefined) return undefined;
    const settings = maintConfig(raw);
    const target = settings.holdings.find(h => h.holdingSlug === actor.holdingSlug && h.holdingId === actor.holdingId);
    if (!target || !actor.authority || !actor.iam) return maintFail(409, 'WORK_CONTEXT_CHANGED');
    const incoming = await headers();
    const host = incoming.get('host');
    if (!host || new URL(target.origin).host !== host) return maintFail(409, 'WORK_CONTEXT_CHANGED');
    const h = new Headers({ host, accept: 'application/json', 'cache-control': 'no-cache' });
    const cookie = incoming.get('cookie');
    if (cookie) {
        if (new TextEncoder().encode(cookie).length > 8192) return maintFail(503, 'NATIVE_MAINTENANCE_SOURCE_RECORD_COOKIE_LIMIT');
        h.set('cookie', cookie);
    }
    h.set('x-erpx-context-company', actor.companyId);
    h.set('x-erpx-context-assignment', actor.assignmentId);
    h.set('x-erpx-context-revision', String(actor.authority.revision));
    h.set('x-erpx-context-assignment-version', String(actor.authority.assignmentVersion));
    if (actor.scope.kind !== 'holding' && actor.scope.branchSlug) h.set('x-erpx-workspace-branch', actor.scope.branchSlug);
    if (actor.scope.kind === 'department') h.set('x-erpx-workspace-department', actor.scope.departmentSlug);
    const url = new URL(`/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/${kind === 'maintenance_plan' ? 'maintenance/plans' : 'maintenance'}`, settings.privateBaseUrl);
    url.searchParams.set('id', id);
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), settings.timeoutMs);
    let response: Response;
    try { response = await fetch(url, { method: 'GET', headers: h, signal: controller.signal, redirect: 'error', cache: 'no-store' }); }
    catch { return maintFail(503, 'NATIVE_MAINTENANCE_SOURCE_RECORD_UNAVAILABLE'); }
    finally { clearTimeout(timer); }
    if (response.status === 403 || response.status === 404) {
        void response.body?.cancel().catch(() => undefined);
        return maintFail(response.status, 'SOURCE_RECORD_NOT_FOUND');
    }
    if (!response.ok || response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
        void response.body?.cancel().catch(() => undefined);
        return maintFail(503, 'NATIVE_MAINTENANCE_SOURCE_RECORD_UNAVAILABLE');
    }
    let parsed: unknown;
    try { parsed = await response.json(); } catch { return maintFail(503, 'NATIVE_MAINTENANCE_SOURCE_RECORD_RESPONSE_INVALID'); }
    if (!maintObject(parsed) || !maintObject(parsed.data) || !maintObject(parsed.context))
        return maintFail(503, 'NATIVE_MAINTENANCE_SOURCE_RECORD_RESPONSE_INVALID');
    const context = parsed.context;
    if (context.companyId !== actor.companyId || context.assignmentId !== actor.assignmentId ||
        context.revision !== actor.authority.revision || context.assignmentVersion !== actor.authority.assignmentVersion ||
        !Array.isArray(context.permissions) ||
        !actor.iam.permissions.every(permission => (context.permissions as unknown[]).includes(permission)))
        return maintFail(409, 'WORK_CONTEXT_CHANGED');
    const row = kind === 'maintenance_plan' ? parsed.data.plan : parsed.data.work;
    if (!maintObject(row) || !maintText(row.id) || !maintText(row.company_id) ||
        typeof row.version !== 'number' || !Number.isSafeInteger(row.version) ||
        typeof row.title !== 'string' || typeof row.status !== 'string')
        return maintFail(503, 'NATIVE_MAINTENANCE_SOURCE_RECORD_RESPONSE_INVALID');
    if (row.id !== id || row.company_id !== actor.companyId) return maintFail(404, 'SOURCE_RECORD_NOT_FOUND');
    if (version !== undefined && row.version !== version) return maintFail(409, 'SOURCE_RECORD_CHANGED');
    const branchId = row.branch_id, departmentId = row.department_id;
    if (!(branchId === null || typeof branchId === 'string') || !(departmentId === null || typeof departmentId === 'string'))
        return maintFail(503, 'NATIVE_MAINTENANCE_SOURCE_RECORD_RESPONSE_INVALID');
    return { id: row.id, company_id: row.company_id, branch_id: branchId, department_id: departmentId,
        version: row.version, title: row.title, status: row.status };
}
function application(provided?: SqlDatabase, requestedVersion?: number): ReturnType<typeof createSourceRecordApplication<SqlStatement>> {
    let database = provided!;
    return createSourceRecordApplication<SqlStatement>({ clock: { now: Date.now }, prepare: async (holding) => { database = provided ?? (await holdingResources(holding)).write; }, authorize: (a, permission) => fromNativeSync(() => requirePermission(a, permission)), taskGrant: async (a, now) => !!await database.prepare(`SELECT 1 FROM (${identity_accessProjectionSources.assignments}) assignments WHERE id=? AND company_id=? AND user_id=? AND role=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?) AND iam_role_allows(role,company_id,'records.read')`).bind(a.assignmentId, a.companyId, a.userId, a.role, now, now).first(), task: (a, id) => fromNative(() => getTask(database, a, id)), pms: (a, id) => fromNative(() => readablePmsSource(database, a, id)), pmsLink: pmsSourceLink, support: (a, id) => fromNative(() => getSupportTicket(a, id, database)), source: (a, kind, id) => {
            switch (kind) {
                case 'commerce_order': return commerceSource(database).read(a, kind, id);
                case 'field_service_order': return fromNative(async () => {
                    if (provided) throw new BusinessRuleError(503, 'FIELD_RECORD_SOURCE_NATIVE_UNIT_REQUIRED', 'This Field-linked operation requires the native Records transaction.');
                    const record = await nativeSourceRecord(a, id, requestedVersion);
                    if (record === undefined) throw new BusinessRuleError(503, 'FIELD_RECORD_SOURCE_NATIVE_UNAVAILABLE', 'The native Field source is unavailable.');
                    return record;
                });
                case 'pos_sale': return posSource(database).read(a, kind, id);
                case 'asset': return assetsSource(database).read(a, kind, id);
                case 'maintenance_plan':
                case 'maintenance_work': return fromNative(async () => {
                    if (provided) throw new BusinessRuleError(503, 'MAINTENANCE_RECORD_SOURCE_NATIVE_UNIT_REQUIRED', 'This Maintenance-linked operation requires the native Records transaction.');
                    const record = await nativeMaintenanceSourceRecord(a, kind, id, requestedVersion);
                    if (record === undefined) throw new BusinessRuleError(503, 'MAINTENANCE_RECORD_SOURCE_NATIVE_UNAVAILABLE', 'The native Maintenance source is unavailable.');
                    return record;
                });
            }
        }, taskGuard: (a, id, version, key, step) => taskSourceReadGuard(database, a, id, version, key, step), postGuard: async (a, id, key, step) => sourceGuard(database, key, await fromNative(() => readablePost(database, a, id)), a.userId, step), pmsGuard: (a, key, source, step) => assertion(database, key, step, `EXISTS(SELECT 1 FROM (${pmsSourcesSql}) s WHERE s.id=? AND s.company_id=? AND s.version=? AND ${pmsSourceReadableSql("s", "?::text", "?::text")})`, [source.id, a.companyId, source.version, a.userId, a.assignmentId]), supportGuards: (a, key, source, step) => fromNative(() => supportTicketSourceGuards(database, a, key, source.id, source.version, step)), sourceGuards: (a, key, source, step) => {
            switch (source.kind) {
                case 'commerce_order': return commerceSource(database).guards(a, key, { ...source, kind: source.kind }, step);
                case 'field_service_order': throw new BusinessRuleError(503, 'FIELD_RECORD_SOURCE_NATIVE_UNIT_REQUIRED', 'This Field-linked operation requires the native Records transaction.');
                case 'pos_sale': return posSource(database).guards(a, key, { ...source, kind: source.kind }, step);
                case 'asset': return assetsSource(database).guards(a, key, { ...source, kind: source.kind }, step);
                case 'maintenance_plan':
                case 'maintenance_work': throw new BusinessRuleError(503, 'MAINTENANCE_RECORD_SOURCE_NATIVE_UNIT_REQUIRED', 'This Maintenance-linked operation requires the native Records transaction.');
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
export async function readSourceRecord(a: WorkContext, raw: z.infer<typeof sourceRecordInput>, db?: SqlDatabase): Promise<AuthorizedSourceRecord> { return invoke(() => application(db,raw.version).readSourceRecord(a, raw)); }
export async function visibleSourceRecord(a: WorkContext, input: z.infer<typeof sourceRecordInput>, db?: SqlDatabase): Promise<AuthorizedSourceRecord | null> { return invoke(() => application(db,input.version).visibleSourceRecord(a, input)); }
export async function sourceRecordGuards(db: SqlDatabase, a: WorkContext, key: string, source: AuthorizedSourceRecord, step = 'source-record'): Promise<SqlStatement[]> { return invoke(() => application(db).sourceRecordGuards(a, key, source, step)); }
