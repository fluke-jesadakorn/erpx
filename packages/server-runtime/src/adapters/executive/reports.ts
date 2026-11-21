import { nativeRuntimeStatement } from '@erpx/feature-runtime/adapters/server/native-runtime-command';
import { executiveReportSources as peopleSources } from '@erpx/server-people_payroll/public/executive-report-sources';
import { executiveReportSources as identitySources } from '@erpx/server-identity_access/public/executive-report-sources';
import "server-only";
import { z } from "zod";
import { shareSchema, sharePreviewSchema, transferOwnershipSchema, type Filters, type ReportView } from "@erpx/feature-shared/lib/executive/canvas";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError } from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { currentAuthority, canAuthor, requireAuthor, executiveEligible } from "@erpx/server-runtime/public/executive/access";
import { metricBreakdowns, metricCatalogFor, metricSupportsComparison, metricUnavailable, queryMetric } from "@erpx/server-runtime/public/executive/metrics";
import { htmlWidgetDocument } from "@erpx/server-runtime/public/executive/html-widget";
import { loadIamAuthority, iamRoles } from "@erpx/server-identity_access/public/iam-store";
import { betaAccess } from "@erpx/server-identity_access/public/beta-access";
import type { SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { type ReportRow, type ShareRow, saveSchema } from '@erpx/server-runtime/domain/executive/reports';
import { createExecutiveReportApplication, type ExecutiveReportRepository } from '@erpx/server-runtime/application/executive/reports';
import { BusinessRuleError } from '@erpx/server-shared/index';
function application(database: SqlDatabase) {
    const repository: ExecutiveReportRepository<SqlStatement> = { ...peopleSources(database), ...identitySources(database), readReportAccessR(id: string, companyId: string) { return database.prepare("SELECT * FROM executive_reports WHERE id=? AND company_id=?").bind(id, companyId).first<ReportRow>(); }, readReportAccessShare(id: string, companyId: string, userId: string, now: number) { return database.prepare("SELECT * FROM executive_report_shares WHERE report_id=? AND company_id=? AND recipient_id=? AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>?)").bind(id, companyId, userId, now).first<ShareRow>(); }, readReportViewShares(id: string) { return database.prepare("SELECT * FROM executive_report_shares WHERE report_id=? AND revoked_at IS NULL ORDER BY created_at").bind(id).all<ShareRow>(); }, readReportViewRevisions(id: string) {
            return database.prepare("SELECT version,created_at FROM executive_report_revisions WHERE report_id=? ORDER BY version DESC LIMIT 30").bind(id).all<{
                version: number;
                created_at: number;
            }>();
        }, readReportLibraryRows(userId: string, now: number, companyId: string, userId2: string, value: 0 | 1) {
            return database.prepare(`SELECT r.*,s.definition_json shared_definition FROM executive_reports r LEFT JOIN executive_report_shares s ON s.report_id=r.id AND s.recipient_id=? AND s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at>?) WHERE r.company_id=? AND ((r.owner_id=? AND ?=1) OR s.id IS NOT NULL) ORDER BY r.updated_at DESC LIMIT 200`).bind(userId, now, companyId, userId2, value).all<ReportRow & {
                shared_definition: string | null;
            }>();
        }, stageCreateReportEffect(id: string, companyId: string, userId: string, value: string, value2: string, now: number, now2: number) { return nativeRuntimeStatement('runtime_stmt_1', { companyId: companyId, now: now, actor: undefined }).bind(id, companyId, userId, value, value2, now, now2); }, stageCreateReportEffect2(id: string, value: string, value2: string, userId: string, now: number) { return nativeRuntimeStatement('runtime_stmt_2', { companyId: '', now: now, actor: undefined }).bind(id, value, value2, userId, now); }, stageSaveReportEffect(id: string) { return database.prepare("SELECT id FROM executive_reports WHERE id=? FOR UPDATE").bind(id); }, guard11(key: string, value: "report-version", id: string, expectedVersion: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM executive_reports WHERE id=? AND version=?)", [id, expectedVersion]); }, guard12(key: string, value: "report-share", id: string, version: number, now: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM executive_report_shares WHERE id=? AND version=? AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>?))", [id, version, now]); }, stageSaveReportEffect2(value: string, value2: string, version: number, now: number, id: string) { return nativeRuntimeStatement('runtime_stmt_3', { companyId: '', now: now, actor: undefined }).bind(value, value2, version, now, id); }, stageSaveReportEffect3(id: string, version: number, value: string, value2: string, userId: string, now: number) { return nativeRuntimeStatement('runtime_stmt_4', { companyId: '', now: now, actor: undefined }).bind(id, version, value, value2, userId, now); }, stageSaveReportEffect4(value: string, id: string) { return nativeRuntimeStatement('runtime_stmt_5', { companyId: '', now: Date.now(), actor: undefined }).bind(value, id); }, readRestoreReportR(id: string, version: number) {
            return database.prepare("SELECT definition_json,filters_json FROM executive_report_revisions WHERE report_id=? AND version=?").bind(id, version).first<{
                definition_json: string;
                filters_json: string;
            }>();
        }, stageSaveShareEffect(id: string) { return database.prepare("SELECT id FROM executive_reports WHERE id=? FOR UPDATE").bind(id); }, guard20(key: string, value: "share-report-version", id: string, userId: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM executive_reports WHERE id=? AND owner_id=? AND version=?)", [id, userId, version]); }, stageSaveShareEffect2(shareId: string, id: string, companyId: string, recipientId: string, legal_name: string, role: "viewer" | "editor", mode: "recipient" | "distribution", value: string, value2: string, value3: string, value4: string, expiresAt: number | null, now: number) { return nativeRuntimeStatement('runtime_stmt_6', { companyId: companyId, now: now, actor: undefined }).bind(shareId, id, companyId, recipientId, legal_name, role, mode, value, value2, value3, value4, expiresAt, now); }, readOwnershipCandidatesShares(id: string) {
            return database.prepare("SELECT id,version FROM executive_report_shares WHERE report_id=? AND revoked_at IS NULL ORDER BY id").bind(id).all<{
                id: string;
                version: number;
            }>();
        }, stageTransferReportOwnershipEffect(id: string) { return database.prepare("SELECT id FROM executive_reports WHERE id=? FOR UPDATE").bind(id); }, guard32(key: string, value: "transfer-report-version", id: string, userId: string, expectedVersion: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM executive_reports WHERE id=? AND owner_id=? AND version=?)", [id, userId, expectedVersion]); }, guard34(key: string, value: "transfer-share-set", id: string, value2: string) { return assertion(database, key, value, "COALESCE((SELECT jsonb_agg(jsonb_build_object('id',id,'version',version) ORDER BY id) FROM executive_report_shares WHERE report_id=? AND revoked_at IS NULL),'[]'::jsonb)=?::jsonb", [id, value2]); }, stageTransferReportOwnershipEffect2(ownerId: string, version: number, now: number, id: string) { return nativeRuntimeStatement('runtime_stmt_7', { companyId: '', now: now, actor: undefined }).bind(ownerId, version, now, id); }, stageTransferReportOwnershipEffect3(id: string, version: number, definition_json: string, filters_json: string, userId: string, now: number) { return nativeRuntimeStatement('runtime_stmt_8', { companyId: '', now: now, actor: undefined }).bind(id, version, definition_json, filters_json, userId, now); }, stageTransferReportOwnershipEffect4(now: number, id: string) { return nativeRuntimeStatement('runtime_stmt_9', { companyId: '', now: now, actor: undefined }).bind(now, id); }, stageTransferReportOwnershipEffect5(now: number, id: string) { return nativeRuntimeStatement('runtime_stmt_10', { companyId: '', now: now, actor: undefined }).bind(now, id); }, stageRevokeShareEffect(id: string) { return database.prepare("SELECT id FROM executive_reports WHERE id=? FOR UPDATE").bind(id); }, guard40(key: string, value: "revoke-report-version", id: string, userId: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM executive_reports WHERE id=? AND owner_id=? AND version=?)", [id, userId, version]); }, stageRevokeShareEffect2(now: number, shareId: string, id: string) { return nativeRuntimeStatement('runtime_stmt_11', { companyId: '', now: now, actor: undefined }).bind(now, shareId, id); } };
    return createExecutiveReportApplication<SqlStatement>({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => replayCommand(database, ...args), commit: (...args) => executeCommand(database, ...args) }, currentAuthority: (...args) => fromNative(() => currentAuthority(database, ...args)), canAuthor: (...args) => fromNative(() => canAuthor(database, ...args)), requireAuthor: (...args) => fromNative(() => requireAuthor(database, ...args)), executiveEligible: (...args) => fromNative(() => executiveEligible(database, ...args)), metricBreakdowns: (...args) => fromNativeSync(() => metricBreakdowns(...args)), metricCatalogFor: (...args) => fromNative(() => metricCatalogFor(database, ...args)), metricSupportsComparison: (...args) => fromNativeSync(() => metricSupportsComparison(...args)), metricUnavailable: (...args) => fromNativeSync(() => metricUnavailable(...args)), queryMetric: (...args) => fromNative(() => queryMetric(database, ...args)), htmlWidgetDocument: (...args) => fromNativeSync(() => htmlWidgetDocument(...args)), loadIamAuthority: (...args) => fromNative(() => loadIamAuthority(database, ...args)), iamRoles: (...args) => fromNative(() => iamRoles(database, ...args)), betaAccess: (...args) => fromNative(() => betaAccess(database, ...args)) });
}
async function fromNative<T>(run: () => Promise<T> | T): Promise<T> {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof ApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
function fromNativeSync<T>(run: () => T): T {
    try {
        return run();
    }
    catch (error) {
        if (error instanceof ApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function invoke<T>(run: () => Promise<T>): Promise<T> {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
export { saveSchema } from '@erpx/server-runtime/domain/executive/reports';
export async function reportAccess(db: SqlDatabase, a: WorkContext, id: string) { return invoke(() => application(db).reportAccess(a, id)); }
export async function reportView(db: SqlDatabase, a: WorkContext, id: string): Promise<ReportView> { return invoke(() => application(db).reportView(a, id)); }
export async function reportLibrary(db: SqlDatabase, a: WorkContext) { return invoke(() => application(db).reportLibrary(a)); }
export async function createReport(db: SqlDatabase, a: WorkContext, key: string, title = "Untitled report") { return invoke(() => application(db).createReport(a, key, title)); }
export async function saveReport(db: SqlDatabase, a: WorkContext, id: string, input: z.infer<typeof saveSchema>, key: string) { return invoke(() => application(db).saveReport(a, id, input, key)); }
export async function restoreReport(db: SqlDatabase, a: WorkContext, id: string, version: number, expectedVersion: number, key: string) { return invoke(() => application(db).restoreReport(a, id, version, expectedVersion, key)); }
export async function recipients(db: SqlDatabase, a: WorkContext, id: string, q: string) { return invoke(() => application(db).recipients(a, id, q)); }
export async function saveShare(db: SqlDatabase, a: WorkContext, id: string, input: z.infer<typeof shareSchema>, key: string) { return invoke(() => application(db).saveShare(a, id, input, key)); }
export async function recipientPreviewContexts(db: SqlDatabase, a: WorkContext, id: string, recipientId: string) { return invoke(() => application(db).recipientPreviewContexts(a, id, recipientId)); }
export async function previewShare(db: SqlDatabase, a: WorkContext, id: string, input: z.infer<typeof sharePreviewSchema>) { return invoke(() => application(db).previewShare(a, id, input)); }
export async function ownershipCandidates(db: SqlDatabase, a: WorkContext, id: string) { return invoke(() => application(db).ownershipCandidates(a, id)); }
export async function transferReportOwnership(db: SqlDatabase, a: WorkContext, id: string, input: z.infer<typeof transferOwnershipSchema>, key: string) { return invoke(() => application(db).transferReportOwnership(a, id, input, key)); }
export async function revokeShare(db: SqlDatabase, a: WorkContext, id: string, shareId: string, key: string) { return invoke(() => application(db).revokeShare(a, id, shareId, key)); }
export async function reportData(db: SqlDatabase, a: WorkContext, id: string, filters?: Filters) { return invoke(() => application(db).reportData(a, id, filters)); }
