import { supportEvidenceEvent } from "../../../support/public/support-evidence";
import "server-only";
import { createHash, randomUUID } from "node:crypto";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/workspace";
import { executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { ApiError } from "@/server/modules/runtime/public/http";
import type { FileStore, SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { isTransactionalSqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { attachmentUploadsFrozen, attachmentsV2Enabled, attachmentsV2ReadOnly, requireAttachmentsV2, requireAttachmentsV2Read } from "@/server/attachments/feature";
import { authorizeAttachmentTarget } from "@/server/attachments/policy";
import { attachmentPreviewReadiness, detectedAttachmentMediaType, sanitizedAttachmentPreview } from "@/server/attachments/preview";
import { portalCollaborationAttachmentRecordVersionStep, portalCollaborationAttachmentTargetVersionGuard } from "@/server/attachments/portal-collaboration-policy";
import { attachmentScannerReadiness, scanAttachmentBytes } from "@/server/attachments/scanner";
import { sensitiveAttachmentRecordVersionStep, sensitiveAttachmentTargetVersionGuard } from "@/server/attachments/sensitive-policy";
import { financeOperationsAttachmentRecordVersionStep, financeOperationsAttachmentTargetVersionGuard } from "@/server/attachments/finance-operations-policy";
import { remainingOperationalAttachmentRecordVersionStep, remainingOperationalAttachmentTargetVersionGuard } from "@/server/attachments/remaining-operational-policy";
import type { AttachmentBindingContract } from "@/server/attachments/types";
import type { AttachmentComponentKind } from "@/server/attachments/types";
import { createAttachmentService, type AttachmentServiceUnit } from "../../application/attachments/service";
import type { AttachmentByteStore } from "../../domain/file-object";
import { repository } from "./service-repository";
import { attachmentServiceOperationsFacts } from "../../../operations/public/attachments-service-facts";
import { attachmentServicePeoplePayrollFacts } from "../../../people_payroll/public/attachments-service-facts";
import { attachmentServiceCommerceFacts } from "../../../commerce/public/attachments-service-facts";
import { attachmentServiceLegalFacts } from "../../../legal/public/attachments-service-facts";
import { attachmentServicePlanningFacts } from "../../../planning/public/attachments-service-facts";
import { attachmentServiceQualityFacts } from "../../../quality/public/attachments-service-facts";
import { attachmentServiceInventoryFacts } from "../../../inventory/public/attachments-service-facts";
import { attachmentServiceCollaborationFacts } from "../../../collaboration/public/attachments-service-facts";
import { attachmentServiceCrmFacts } from "../../../crm/public/attachments-service-facts";
import { attachmentServiceProjectsFacts } from "../../../projects/public/attachments-service-facts";
import { attachmentServiceMaintenanceFacts } from "../../../maintenance/public/attachments-service-facts";
import { attachmentServiceRecordsFacts } from "../../../records/public/attachments-service-facts";
import { attachmentServiceFinanceFacts } from "../../../finance/public/attachments-service-facts";
import { attachmentServiceProcurementFacts } from "../../../procurement/public/attachments-service-facts";
import { attachmentServiceMarketingFacts } from "../../../marketing/public/attachments-service-facts";
import { attachmentServiceOrganizationFacts } from "../../../organization/public/attachments-service-facts";
import { attachmentServiceAssetsFacts } from "../../../assets/public/attachments-service-facts";
import { attachmentServiceSupportFacts } from "../../../support/public/attachments-service-facts";
import { attachmentServiceFieldServiceFacts } from "../../../field_service/public/attachments-service-facts";
import { attachmentServiceManufacturingFacts } from "../../../manufacturing/public/attachments-service-facts";
import { attachmentServiceIdentityAccessFacts } from "../../../identity_access/public/attachments-service-facts";
import { attachmentServiceManageErpxFacts } from "../../../manage_erpx/public/attachments-service-facts";
import { attachmentServiceBrandFacts } from "../../../brand/public/attachments-service-facts";
const nativeDatabases = new WeakMap<AttachmentServiceUnit<SqlStatement>, SqlDatabase>();
function unit(db: SqlDatabase): AttachmentServiceUnit<SqlStatement> {
    const context: AttachmentServiceUnit<SqlStatement> = { ownerVersionGuard: (...args) => { const [commandId, targetType, targetId, actor, version, index] = args, companyId = actor.companyId; const special = portalCollaborationAttachmentTargetVersionGuard(db, commandId, targetType, targetId, companyId, version, index) ?? sensitiveAttachmentTargetVersionGuard(db, commandId, targetType, targetId, companyId, version, index) ?? financeOperationsAttachmentTargetVersionGuard(db, commandId, targetType, targetId, companyId, version, index) ?? remainingOperationalAttachmentTargetVersionGuard(db, commandId, targetType, targetId, companyId, version, index); if (special)
            return special; const effect = attachmentServiceOperationsFacts(db).versionGuard(...args) ?? attachmentServicePeoplePayrollFacts(db).versionGuard(...args) ?? attachmentServiceCommerceFacts(db).versionGuard(...args) ?? attachmentServiceLegalFacts(db).versionGuard(...args) ?? attachmentServicePlanningFacts(db).versionGuard(...args) ?? attachmentServiceQualityFacts(db).versionGuard(...args) ?? attachmentServiceInventoryFacts(db).versionGuard(...args) ?? attachmentServiceCollaborationFacts(db).versionGuard(...args) ?? attachmentServiceCrmFacts(db).versionGuard(...args) ?? attachmentServiceProjectsFacts(db).versionGuard(...args) ?? attachmentServiceMaintenanceFacts(db).versionGuard(...args) ?? attachmentServiceRecordsFacts(db).versionGuard(...args) ?? attachmentServiceFinanceFacts(db).versionGuard(...args) ?? attachmentServiceProcurementFacts(db).versionGuard(...args) ?? attachmentServiceMarketingFacts(db).versionGuard(...args) ?? attachmentServiceOrganizationFacts(db).versionGuard(...args) ?? attachmentServiceAssetsFacts(db).versionGuard(...args) ?? attachmentServiceSupportFacts(db).versionGuard(...args) ?? attachmentServiceFieldServiceFacts(db).versionGuard(...args) ?? attachmentServiceManufacturingFacts(db).versionGuard(...args); if (!effect)
            throw new ApiError(422, "ATTACHMENT_TARGET_UNSUPPORTED", "This file target has no atomic version guard."); return effect; },
        ownerVersionStep: (...args) => { const special = portalCollaborationAttachmentRecordVersionStep(db, ...args) ?? sensitiveAttachmentRecordVersionStep(db, ...args) ?? financeOperationsAttachmentRecordVersionStep(db, ...args) ?? remainingOperationalAttachmentRecordVersionStep(db, ...args); if (special)
            return special; const effect = attachmentServiceOperationsFacts(db).versionStep(...args) ?? attachmentServicePeoplePayrollFacts(db).versionStep(...args) ?? attachmentServiceCommerceFacts(db).versionStep(...args) ?? attachmentServiceLegalFacts(db).versionStep(...args) ?? attachmentServicePlanningFacts(db).versionStep(...args) ?? attachmentServiceQualityFacts(db).versionStep(...args) ?? attachmentServiceInventoryFacts(db).versionStep(...args) ?? attachmentServiceCollaborationFacts(db).versionStep(...args) ?? attachmentServiceCrmFacts(db).versionStep(...args) ?? attachmentServiceProjectsFacts(db).versionStep(...args) ?? attachmentServiceMaintenanceFacts(db).versionStep(...args) ?? attachmentServiceRecordsFacts(db).versionStep(...args) ?? attachmentServiceFinanceFacts(db).versionStep(...args) ?? attachmentServiceProcurementFacts(db).versionStep(...args) ?? attachmentServiceMarketingFacts(db).versionStep(...args) ?? attachmentServiceOrganizationFacts(db).versionStep(...args) ?? attachmentServiceAssetsFacts(db).versionStep(...args) ?? attachmentServiceSupportFacts(db).versionStep(...args) ?? attachmentServiceFieldServiceFacts(db).versionStep(...args) ?? attachmentServiceManufacturingFacts(db).versionStep(...args); if (!effect)
            throw new ApiError(422, "RECORD_ATTACHMENT_BIND_UNSUPPORTED", "This record does not support standalone document binding."); return effect; },
        readAttachmentSets1: facts => repository(db).readAttachmentSets1(facts),
        readActiveWorkContexts2: facts => attachmentServiceIdentityAccessFacts(db).readActiveWorkContexts2(facts),
        readAssignments3: facts => attachmentServiceIdentityAccessFacts(db).readAssignments3(facts),
        readLeaveRequests4: facts => attachmentServicePeoplePayrollFacts(db).readLeaveRequests4(facts),
        readAttachmentLinks5: facts => repository(db).readAttachmentLinks5(facts),
        readAttachmentEvents6: facts => repository(db).readAttachmentEvents6(facts),
        readAttachmentSets7: facts => repository(db).readAttachmentSets7(facts),
        lock8: facts => repository(db).lock8(facts),
        readAttachmentSets9: facts => repository(db).readAttachmentSets9(facts),
        readAttachmentEvents10: facts => repository(db).readAttachmentEvents10(facts),
        readAttachmentSets11: facts => repository(db).readAttachmentSets11(facts),
        stepAttachmentSets12: facts => repository(db).stepAttachmentSets12(facts),
        stepAttachmentEvents13: facts => repository(db).stepAttachmentEvents13(facts),
        readAttachmentLinks14: facts => repository(db).readAttachmentLinks14(facts),
        lock15: facts => repository(db).lock15(facts),
        readUploadIntents16: facts => repository(db).readUploadIntents16(facts),
        readAttachmentLinks17: facts => repository(db).readAttachmentLinks17(facts),
        readAttachmentStorageLedger18: facts => repository(db).readAttachmentStorageLedger18(facts),
        readUploadIntents19: facts => repository(db).readUploadIntents19(facts),
        readBetaPolicy20: facts => attachmentServiceManageErpxFacts(db).readBetaPolicy20(facts),
        stepAttachmentLinks21: facts => repository(db).stepAttachmentLinks21(facts),
        stepUploadIntents22: facts => repository(db).stepUploadIntents22(facts),
        stepAttachmentLinks23: facts => repository(db).stepAttachmentLinks23(facts),
        stepAttachmentEvents24: facts => repository(db).stepAttachmentEvents24(facts),
        readUploadIntents25: facts => repository(db).readUploadIntents25(facts),
        readAttachments26: facts => repository(db).readAttachments26(facts),
        lock27: facts => repository(db).lock27(facts),
        readUploadIntents28: facts => repository(db).readUploadIntents28(facts),
        readAttachments29: facts => repository(db).readAttachments29(facts),
        readAttachmentLinks30: facts => repository(db).readAttachmentLinks30(facts),
        readAttachmentLinks31: facts => repository(db).readAttachmentLinks31(facts),
        readAttachmentSets32: facts => repository(db).readAttachmentSets32(facts),
        stepAttachments33: facts => repository(db).stepAttachments33(facts),
        stepUploadIntents34: facts => repository(db).stepUploadIntents34(facts),
        stepAttachmentLinks35: facts => repository(db).stepAttachmentLinks35(facts),
        stepAttachmentScanAttempts36: facts => repository(db).stepAttachmentScanAttempts36(facts),
        stepAttachmentStorageLedger37: facts => repository(db).stepAttachmentStorageLedger37(facts),
        stepAttachmentEvents38: facts => repository(db).stepAttachmentEvents38(facts),
        readAttachmentLinks39: facts => repository(db).readAttachmentLinks39(facts),
        readAttachmentScanAttempts40: facts => repository(db).readAttachmentScanAttempts40(facts),
        writeAttachmentAccessEvents41: facts => repository(db).writeAttachmentAccessEvents41(facts),
        readAttachmentLinks42: facts => repository(db).readAttachmentLinks42(facts),
        writeAttachmentAccessEvents43: facts => repository(db).writeAttachmentAccessEvents43(facts),
        writeAttachmentAccessEvents44: facts => repository(db).writeAttachmentAccessEvents44(facts),
        readAttachmentLinks45: facts => repository(db).readAttachmentLinks45(facts),
        readAttachmentLinks46: facts => repository(db).readAttachmentLinks46(facts),
        stepAttachmentLinks47: facts => repository(db).stepAttachmentLinks47(facts),
        stepAttachmentEvents48: facts => repository(db).stepAttachmentEvents48(facts),
        lock49: facts => repository(db).lock49(facts),
        readAttachmentEvents50: facts => repository(db).readAttachmentEvents50(facts),
        readAttachmentSets51: facts => repository(db).readAttachmentSets51(facts),
        readAttachmentLinks52: facts => repository(db).readAttachmentLinks52(facts),
        stepAttachmentEvents53: facts => repository(db).stepAttachmentEvents53(facts),
        stepAttachmentSets54: facts => repository(db).stepAttachmentSets54(facts),
        readAttachmentLinks55: facts => repository(db).readAttachmentLinks55(facts),
        lock56: facts => repository(db).lock56(facts),
        readAttachmentSets57: facts => repository(db).readAttachmentSets57(facts),
        readAttachmentLinks58: facts => repository(db).readAttachmentLinks58(facts),
        readAttachmentLinks59: facts => repository(db).readAttachmentLinks59(facts),
        readAttachmentLinks60: facts => repository(db).readAttachmentLinks60(facts),
        stepAttachmentLinks61: facts => repository(db).stepAttachmentLinks61(facts),
        stepAttachmentEvents62: facts => repository(db).stepAttachmentEvents62(facts),
        readAttachmentSets63: facts => repository(db).readAttachmentSets63(facts),
        readAttachmentLinks64: facts => repository(db).readAttachmentLinks64(facts),
        readAttachmentEvents65: facts => repository(db).readAttachmentEvents65(facts),
        stepAttachmentLinks66: facts => repository(db).stepAttachmentLinks66(facts),
        stepAttachmentSets67: facts => repository(db).stepAttachmentSets67(facts),
        stepAttachmentEvents68: facts => repository(db).stepAttachmentEvents68(facts),
        stepOrganizationBrandProfiles71: facts => attachmentServiceBrandFacts(db).stepOrganizationBrandProfiles71(facts),
        stepStorefrontAppBrands72: facts => attachmentServiceBrandFacts(db).stepStorefrontAppBrands72(facts),
        stepOrganizationBrandProfiles73: facts => attachmentServiceBrandFacts(db).stepOrganizationBrandProfiles73(facts),
        stepStorefrontAppBrands74: facts => attachmentServiceBrandFacts(db).stepStorefrontAppBrands74(facts),
        readAttachmentLinks75: facts => repository(db).readAttachmentLinks75(facts),
        readAttachmentPublications76: facts => repository(db).readAttachmentPublications76(facts),
        lock77: facts => repository(db).lock77(facts),
        readAttachmentPublications78: facts => repository(db).readAttachmentPublications78(facts),
        readAttachmentPublications79: facts => repository(db).readAttachmentPublications79(facts),
        stepAttachmentPublications80: facts => repository(db).stepAttachmentPublications80(facts),
        stepAttachmentPublicationOrders81: facts => repository(db).stepAttachmentPublicationOrders81(facts),
        stepAttachmentStorageLedger82: facts => repository(db).stepAttachmentStorageLedger82(facts),
        stepAttachmentEvents83: facts => repository(db).stepAttachmentEvents83(facts),
        readAttachmentPublications84: facts => repository(db).readAttachmentPublications84(facts),
        readAttachmentPublications85: facts => repository(db).readAttachmentPublications85(facts),
        readAttachmentPublicationOrders86: facts => repository(db).readAttachmentPublicationOrders86(facts),
        readAttachmentPublications87: facts => repository(db).readAttachmentPublications87(facts),
        readAttachmentPublications88: facts => repository(db).readAttachmentPublications88(facts),
        stepAttachmentPublications89: facts => repository(db).stepAttachmentPublications89(facts),
        stepAttachmentEvents90: facts => repository(db).stepAttachmentEvents90(facts),
        lock92: facts => repository(db).lock92(facts),
        readAttachmentEvents93: facts => repository(db).readAttachmentEvents93(facts),
        readAttachmentPublicationOrders94: facts => repository(db).readAttachmentPublicationOrders94(facts),
        readAttachmentPublications95: facts => repository(db).readAttachmentPublications95(facts),
        stepAttachmentPublications96: facts => repository(db).stepAttachmentPublications96(facts),
        stepAttachmentPublicationOrders97: facts => repository(db).stepAttachmentPublicationOrders97(facts),
        stepAttachmentEvents98: facts => repository(db).stepAttachmentEvents98(facts),
        readAttachmentPublications99: facts => repository(db).readAttachmentPublications99(facts),
        readAttachmentPublications100: facts => repository(db).readAttachmentPublications100(facts),
        stepAttachmentPublications102: facts => repository(db).stepAttachmentPublications102(facts),
        stepAttachmentEvents103: facts => repository(db).stepAttachmentEvents103(facts),
        readAttachmentPublications105: facts => repository(db).readAttachmentPublications105(facts),
        readAttachmentScanAttempts106: facts => repository(db).readAttachmentScanAttempts106(facts),
        readAttachments107: facts => repository(db).readAttachments107(facts),
        writeAttachments108: facts => repository(db).writeAttachments108(facts),
        stepAttachmentScanAttempts109: facts => repository(db).stepAttachmentScanAttempts109(facts),
        stepAttachments110: facts => repository(db).stepAttachments110(facts),
        stepAttachmentEvents111: facts => repository(db).stepAttachmentEvents111(facts),
        stepAttachmentScanAttempts112: facts => repository(db).stepAttachmentScanAttempts112(facts),
        stepAttachments113: facts => repository(db).stepAttachments113(facts),
        stepUploadIntents114: facts => repository(db).stepUploadIntents114(facts),
        stepAttachmentLinks115: facts => repository(db).stepAttachmentLinks115(facts),
        stepAttachmentEvents116: facts => repository(db).stepAttachmentEvents116(facts),
        stepAttachmentStorageLedger117: facts => repository(db).stepAttachmentStorageLedger117(facts),
        stepAttachmentScanAttempts118: facts => repository(db).stepAttachmentScanAttempts118(facts),
        stepAttachments119: facts => repository(db).stepAttachments119(facts),
        stepAttachmentEvents120: facts => repository(db).stepAttachmentEvents120(facts),
        stepAttachmentScanAttempts121: facts => repository(db).stepAttachmentScanAttempts121(facts),
        readAttachments122: facts => repository(db).readAttachments122(facts),
        readAttachments123: facts => repository(db).readAttachments123(facts),
        readAttachmentLinks124: facts => repository(db).readAttachmentLinks124(facts),
        readLegalFilePreservations125: facts => attachmentServiceLegalFacts(db).readLegalFilePreservations125(facts),
        readAttachmentSets126: facts => repository(db).readAttachmentSets126(facts),
        readAttachmentLinks127: facts => repository(db).readAttachmentLinks127(facts),
        writeAttachmentEvents128: facts => repository(db).writeAttachmentEvents128(facts),
        writeUploadIntents129: facts => repository(db).writeUploadIntents129(facts),
        readAttachmentSets130: facts => repository(db).readAttachmentSets130(facts),
        writeAttachmentEvents131: facts => repository(db).writeAttachmentEvents131(facts),
        readLeaveRequests132: facts => attachmentServicePeoplePayrollFacts(db).readLeaveRequests132(facts),
        readAttachmentLinks133: facts => repository(db).readAttachmentLinks133(facts),
        writeAttachmentEvents134: facts => repository(db).writeAttachmentEvents134(facts),
        writeUploadIntents135: facts => repository(db).writeUploadIntents135(facts),
        readAttachmentSets136: facts => repository(db).readAttachmentSets136(facts),
        writeAttachmentEvents137: facts => repository(db).writeAttachmentEvents137(facts),
        readAttachments138: facts => repository(db).readAttachments138(facts),
        lock139: facts => repository(db).lock139(facts),
        lock140: () => repository(db).lock140(),
        readAttachments141: facts => repository(db).readAttachments141(facts),
        readAttachments142: facts => repository(db).readAttachments142(facts),
        writeAttachmentStorageLedger143: facts => repository(db).writeAttachmentStorageLedger143(facts),
        writeAttachmentEvents144: facts => repository(db).writeAttachmentEvents144(facts),
        readAttachments145: () => repository(db).readAttachments145(),
        readAttachments148: facts => repository(db).readAttachments148(facts),
        readAttachmentEvents149: facts => repository(db).readAttachmentEvents149(facts),
        readAttachmentStorageLedger150: facts => repository(db).readAttachmentStorageLedger150(facts),
        writeAttachmentEvents151: facts => repository(db).writeAttachmentEvents151(facts),
        writeAttachmentEvents152: facts => repository(db).writeAttachmentEvents152(facts),
        bindingGuard1: facts => repository(db).bindingGuard1(facts),
        bindingGuard2: facts => repository(db).bindingGuard2(facts),
        bindingGuard3: facts => repository(db).bindingGuard3(facts),
        atomic: action => isTransactionalSqlDatabase(db) ? db.transaction(tx => action(unit(tx))) : action(context),
        uploadCleanupLock: key => db.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(key).run(), uploadObjectReferenced: (companyId, key) => db.prepare("SELECT 1 FROM attachments WHERE company_id=? AND object_key=?").bind(companyId, key).first(), supportEvidenceEvent: (...args) => supportEvidenceEvent(db, ...args), batch: effects => db.batch(effects),
        authorizeTarget: (...args) => authorizeAttachmentTarget(db, ...args),
        supportEvidenceGuard: async (...args) => await (await import("../../../support/public/support-evidence")).supportEvidenceGuard(db, ...args),
        replayCommand: (...args) => replayCommand(db, ...args),
        executeCommand: (...args) => executeCommand(db, ...args) };
    nativeDatabases.set(context, db);
    return context;
}
function byteStore(files: FileStore): AttachmentByteStore<AttachmentServiceUnit<SqlStatement>> { return { ...files, supportsTransactionDeletion: files.supportsTransactionDeletion, put: (...args) => files.put(...args), get: (...args) => files.get(...args), head: (...args) => files.head(...args), delete: (keys, authorization) => { const database = authorization ? nativeDatabases.get(authorization.unit) : undefined; if (authorization && !database)
        throw new Error("FILES_TRANSACTION_CONTEXT_REQUIRED"); return files.delete(keys, database ? { database } : undefined); } }; }
const service = createAttachmentService<SqlStatement>({ clock: { now: () => Date.now() }, identifiers: { next: randomUUID }, environment: () => process.env, error: (status, code, message) => new ApiError(status, code, message), hash: value => createHash("sha256").update(value).digest("hex"), digest: sha256, isClientError: error => error instanceof ApiError && error.status < 500, errorCode: error => error instanceof ApiError ? error.code : undefined, errorStatus: error => error instanceof ApiError ? error.status : undefined, reportFailure: event => console.error(JSON.stringify(event)), uploadUrl: (actor, token) => `/api/v1${workspaceBase(publicScope(actor))}/uploads/${token}/content`, features: { attachmentUploadsFrozen, attachmentsV2Enabled, attachmentsV2ReadOnly, requireAttachmentsV2, requireAttachmentsV2Read }, providers: { detectedAttachmentMediaType, attachmentScannerReadiness: environment => attachmentScannerReadiness(environment as NodeJS.ProcessEnv), attachmentPreviewReadiness: environment => attachmentPreviewReadiness(environment as NodeJS.ProcessEnv), scanAttachmentBytes: (bytes, environment) => scanAttachmentBytes(bytes, environment as NodeJS.ProcessEnv), sanitizedAttachmentPreview } });
type AttachmentServiceOptions = {
    compatibilityAdapter?: boolean;
    requestId?: string;
};
export function listAttachmentSets(db: SqlDatabase, actor: WorkContext, input: {
    targetType: string;
    targetId: string;
    purposeKey?: string;
    componentKind?: AttachmentComponentKind;
    expectedTargetVersion?: number;
}) { return service.listAttachmentSets(unit(db), actor, input); }
export function createAttachmentSet(db: SqlDatabase, actor: WorkContext, input: {
    targetType: string;
    targetId: string;
    componentKind: AttachmentComponentKind;
    componentRef?: string;
    purposeKey: string;
    expectedTargetVersion: number;
}, idempotencyKey: string, options: AttachmentServiceOptions = {}) { return service.createAttachmentSet(unit(db), actor, input, idempotencyKey, options); }
export function reserveAttachmentFile(db: SqlDatabase, actor: WorkContext, setId: string, input: {
    filename: string;
    mediaType: string;
    bytes: number;
    caption?: string;
    supersedesLinkId?: string;
}, idempotencyKey: string, options: AttachmentServiceOptions = {}) { return service.reserveAttachmentFile(unit(db), actor, setId, input, idempotencyKey, options); }
export function completeAttachmentUpload(db: SqlDatabase, files: FileStore, actor: WorkContext, uploadIntentId: string, buffer: ArrayBuffer, options: AttachmentServiceOptions = {}) { return service.completeAttachmentUpload(unit(db), byteStore(files), actor, uploadIntentId, buffer, options); }
export function attachmentLinkStatus(db: SqlDatabase, actor: WorkContext, linkId: string) { return service.attachmentLinkStatus(unit(db), actor, linkId); }
export function attachmentContent(db: SqlDatabase, files: FileStore, actor: WorkContext, linkId: string, variant: "original" | "preview", options: AttachmentServiceOptions = {}) { return service.attachmentContent(unit(db), byteStore(files), actor, linkId, variant, options); }
export function withdrawAttachmentLink(db: SqlDatabase, actor: WorkContext, linkId: string, reason: string, idempotencyKey: string) { return service.withdrawAttachmentLink(unit(db), actor, linkId, reason, idempotencyKey); }
export function attestAttachmentSetSanitized(db: SqlDatabase, actor: WorkContext, setId: string, input: {
    confirmed: boolean;
    note: string;
}, idempotencyKey: string) { return service.attestAttachmentSetSanitized(unit(db), actor, setId, input, idempotencyKey); }
export function linkExistingAttachment(db: SqlDatabase, actor: WorkContext, setId: string, input: {
    sourceLinkId: string;
    caption?: string;
}, idempotencyKey: string) { return service.linkExistingAttachment(unit(db), actor, setId, input, idempotencyKey); }
export function prepareAttachmentBinding(db: SqlDatabase, actor: WorkContext, input: {
    attachmentSetIds: string[];
    bindingContract: AttachmentBindingContract;
    targetId: string;
    expectedTargetVersion: number;
    commandId: string;
    bindingId: string;
}, options: AttachmentServiceOptions = {}) { return service.prepareAttachmentBinding(unit(db), actor, input, options); }
export function attachmentRecordVersionStep(db: SqlDatabase, targetType: string, targetId: string, expectedVersion: number, now = Date.now()) { return service.attachmentRecordVersionStep(unit(db), targetType, targetId, expectedVersion, now); }
export function bindRecordAttachmentSet(db: SqlDatabase, actor: WorkContext, setId: string, expectedTargetVersion: number, commandId: string, options: AttachmentServiceOptions = {}) { return service.bindRecordAttachmentSet(unit(db), actor, setId, expectedTargetVersion, commandId, options); }
export function bindCompatibilityAttachmentSet(db: SqlDatabase, actor: WorkContext, setId: string, expectedTargetVersion: number, commandId: string, options: AttachmentServiceOptions = {}) { return service.bindCompatibilityAttachmentSet(unit(db), actor, setId, expectedTargetVersion, commandId, options); }
export function createAttachmentPublication(db: SqlDatabase, files: FileStore, actor: WorkContext, input: {
    sourceLinkId: string;
    targetType: string;
    targetId: string;
    altText: string;
    ordinal: number;
}, idempotencyKey: string) { return service.createAttachmentPublication(unit(db), byteStore(files), actor, input, idempotencyKey); }
export function listAttachmentPublications(db: SqlDatabase, actor: WorkContext, targetType: string, targetId: string) { return service.listAttachmentPublications(unit(db), actor, targetType, targetId); }
export function revokeAttachmentPublication(db: SqlDatabase, actor: WorkContext, id: string, reason: string, idempotencyKey: string) { return service.revokeAttachmentPublication(unit(db), actor, id, reason, idempotencyKey); }
export function reorderAttachmentPublications(db: SqlDatabase, actor: WorkContext, input: {
    targetType: string;
    targetId: string;
    expectedVersion: number;
    items: Array<{
        id: string;
        ordinal: number;
    }>;
}, idempotencyKey: string) { return service.reorderAttachmentPublications(unit(db), actor, input, idempotencyKey); }
export function approveAttachmentPublication(db: SqlDatabase, actor: WorkContext, id: string, idempotencyKey: string) { return service.approveAttachmentPublication(unit(db), actor, id, idempotencyKey); }
export function publicAttachmentPublication(db: SqlDatabase, files: FileStore, id: string) { return service.publicAttachmentPublication(unit(db), byteStore(files), id); }
export function drainAttachmentScans(db: SqlDatabase, files: FileStore, attachmentId?: string, options: {
    offlineMigration?: boolean;
} = {}) { return service.drainAttachmentScans(unit(db), byteStore(files), attachmentId, options); }
export function scanAttachmentNow(db: SqlDatabase, files: FileStore, attachmentId: string) { return service.scanAttachmentNow(unit(db), byteStore(files), attachmentId); }
export function attachmentDestructionEligibility(db: SqlDatabase, attachmentId: string, now = Date.now()) { return service.attachmentDestructionEligibility(unit(db), attachmentId, now); }
export function cleanupAttachmentDrafts(db: SqlDatabase, files: FileStore) { return service.cleanupAttachmentDrafts(unit(db), byteStore(files)); }
