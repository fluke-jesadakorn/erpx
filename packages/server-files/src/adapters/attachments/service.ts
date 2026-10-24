import {withPublicationContentRead} from '@erpx/server-files/adapters/publication-content-read';
import {messagingAttachmentPurchaseScope,messagingAttachmentSourceReadLocks,messagingAttachmentFailure,type MessagingAttachmentPurchase} from "@erpx/server-collaboration/public/messaging-attachment-purchases";
import {withWorkTaskAttachmentContentRead} from "@erpx/server-operations/public/task-attachment-read";
import {withPortalAttachmentContentRead} from "@erpx/server-customer_authority/public/attachment-read";
import {flushFinanceDeferredEffects} from "@erpx/server-finance/public/finance-transactions";
import {withEcommerceAttachmentContentRead} from "@erpx/server-ecommerce/public/attachment-read";
import {withFinanceTaxSignatureAttachmentContentRead} from "@erpx/server-finance/public/tax-signature-attachment-read";
import {withMessagingAttachmentContentRead} from "@erpx/server-collaboration/public/messaging-attachment-read";
import {withSupportAttachmentContentRead} from "@erpx/server-support/public/attachment-read";
import {withSupportIncidentAttachmentContentRead} from "@erpx/server-support/public/incident-attachment-read";
import {withServiceAttachmentContentRead} from "@erpx/server-field_service/public/attachment-read";
import {isPeopleAttachmentReadTarget} from '@erpx/server-people_payroll/public/attachment-read-sources';
import {withPeopleAttachmentContentRead} from '@erpx/server-people_payroll/public/attachment-read';
import {isProjectAttachmentReadTarget,withProjectAttachmentContentRead} from '@erpx/server-projects/public/attachment-read';
import {isLearningAttachmentReadTarget,withLearningAttachmentContentRead} from '@erpx/server-learning/public/attachment-read';
import {isCRMAttachmentReadTarget,withCRMAttachmentContentRead} from '@erpx/server-crm/public/attachment-read';
import {isMarketingAttachmentReadTarget,withMarketingAttachmentContentRead} from '@erpx/server-marketing/public/attachment-read';
import {isAssetAttachmentReadTarget,withAssetAttachmentContentRead} from '@erpx/server-assets/public/attachment-read';
import {isInventoryAttachmentReadTarget,withInventoryAttachmentContentRead} from '@erpx/server-inventory/public/attachment-read';
import {isManufacturingAttachmentReadTarget,withManufacturingAttachmentContentRead} from '@erpx/server-manufacturing/public/attachment-read';
import {isCommerceAttachmentReadTarget,withCommerceAttachmentContentRead} from '@erpx/server-commerce/public/attachment-read';
import {isProcurementAttachmentReadTarget,withProcurementAttachmentContentRead} from '@erpx/server-procurement/public/attachment-read';
import {isBenefitAttachmentReadTarget,withBenefitAttachmentContentRead} from '@erpx/server-benefits/public/attachment-read';
import {isQualityAttachmentReadTarget,withQualityAttachmentContentRead} from '@erpx/server-quality/public/attachment-read';
import {withLegalCanonicalAttachmentContentRead} from "@erpx/server-legal/public/canonical-attachment-read";
import {hasLegalEvidenceAttachmentReadParent,withLegalEvidenceAttachmentContentRead} from "@erpx/server-legal/public/evidence-attachment-read";
import {isHospitalityAttachmentReadTarget,withHospitalityAttachmentContentRead} from '@erpx/server-files/adapters/hospitality-attachment-read';
import {isEmployeeEvidenceAttachmentType,withEmployeeEvidenceAttachmentContentRead} from '@erpx/server-files/adapters/employee-evidence-read';
import {isFinanceAttachmentReadTarget} from '@erpx/server-finance/public/attachment-read-sources';
import {withFinanceAttachmentContentRead} from '@erpx/server-finance/public/attachment-read';
import {withSupplierBillAttachmentContentRead} from '@erpx/server-finance/public/supplier-bill-attachment-read';
import {isTalentAttachmentReadTarget,withTalentAttachmentContentRead} from '@erpx/server-talent/public/attachment-read';
import {isBrandAttachmentReadTarget,withBrandAttachmentContentRead} from '@erpx/server-brand/public/attachment-read';
import {isOrganizationAttachmentReadTarget,withOrganizationAttachmentContentRead} from '@erpx/server-organization/public/attachment-read';
import {isIamAttachmentReadTarget,withIamAttachmentContentRead} from '@erpx/server-identity_access/public/attachment-read';
import {isCompanyReleaseAttachmentReadTarget,withCompanyReleaseAttachmentContentRead} from '@erpx/server-platform_administration/public/company-release-attachment-read';
import {isCompanyProvisioningAttachmentReadTarget,withCompanyProvisioningAttachmentContentRead} from '@erpx/server-manage_erpx/public/company-provisioning-attachment-read';
import {isSocialAttachmentReadTarget,withSocialAttachmentContentRead} from '@erpx/server-collaboration/public/social-attachment-read';
import {attachmentReadRoute,attachmentPrivateReadSourceLocks,type AttachmentReadRoute} from "@erpx/server-files/public/private-attachment-reads";
import {ecommerceAttachmentPurchaseScope,ecommerceAttachmentSourceReadLocks,ecommerceAttachmentFailure,type EcommerceAttachmentPurchase} from "@erpx/server-ecommerce/public/attachment-purchases";
import {workTaskAttachmentPurchaseScope,workTaskAttachmentSourceReadLocks,workTaskAttachmentFailure,type WorkTaskAttachmentPurchase} from "@erpx/server-operations/public/task-attachment-purchases";
import {portalAttachmentPurchaseScope,portalAttachmentSourceReadLocks,portalAttachmentFailure,type PortalAttachmentPurchase} from "@erpx/server-customer_authority/public/attachment-purchases";
import {serviceAttachmentPurchaseScope,serviceAttachmentSourceReadLocks,serviceAttachmentFailure,fieldServiceAttachmentRecordVersionStep,type ServiceAttachmentPurchase} from "@erpx/server-field_service/public/attachment-purchases";
import {benefitAttachmentPurchaseScope,benefitAttachmentSourceReadLocks,benefitAttachmentFailure,type BenefitAttachmentPurchase} from "@erpx/server-benefits/public/attachment-purchases";
import {workforceAttachmentPurchaseScope,workforceAttachmentSourceReadLocks,workforceAttachmentFailure,type WorkforceAttachmentPurchase} from "@erpx/server-people_payroll/public/workforce-attachment-purchases";
import {crmAttachmentPurchaseScope,crmAttachmentSourceReadLocks,crmAttachmentFailure,type CRMAttachmentPurchase} from "@erpx/server-crm/public/attachment-purchases";
import {marketingAttachmentPurchaseScope,marketingAttachmentSourceReadLocks,marketingAttachmentFailure,type MarketingAttachmentPurchase} from "@erpx/server-marketing/public/attachment-purchases";
import {manufacturingAttachmentPurchaseScope,manufacturingAttachmentSourceReadLocks,manufacturingAttachmentFailure,type ManufacturingAttachmentPurchase} from "@erpx/server-manufacturing/public/attachment-purchases";
import {legalAttachmentPurchaseScope,legalAttachmentSourceReadLocks,legalAttachmentFailure,type LegalAttachmentPurchase} from "@erpx/server-legal/public/attachment-purchases";
import {talentAttachmentPurchaseScope,talentAttachmentSourceReadLocks,talentAttachmentFailure,type TalentAttachmentPurchase} from "@erpx/server-talent/public/attachment-purchases";
import {landedCostAttachmentPurchaseScope,landedCostAttachmentSourceReadLocks,landedCostAttachmentFailure,type LandedCostAttachmentPurchase} from "@erpx/server-inventory/public/landed-cost-attachment-purchases";
import {assetAttachmentPurchaseScope,assetAttachmentSourceReadLocks,assetAttachmentFailure,type AssetAttachmentPurchase} from "@erpx/server-assets/public/attachment-purchases";
import {learningAttachmentPurchaseScope,learningAttachmentSourceReadLocks,learningAttachmentFailure,type LearningAttachmentPurchase} from "@erpx/server-learning/public/attachment-purchases";
import {qualityAttachmentPurchaseScope,qualityAttachmentSourceReadLocks,qualityAttachmentFailure,type QualityAttachmentPurchase} from "@erpx/server-quality/public/attachment-purchases";
import {projectAttachmentPurchaseScope,projectAttachmentSourceReadLocks,projectAttachmentFailure,type ProjectAttachmentPurchase} from "@erpx/server-projects/public/attachment-purchases";
import {transferAttachmentPurchaseScope,transferAttachmentFailure,type TransferAttachmentPurchase} from "@erpx/server-inventory/public/transfer-attachment-purchases";
import {peopleAttachmentPurchaseScope,peopleAttachmentSourceReadLocks,type PeopleAttachmentPurchase} from "@erpx/server-people_payroll/public/base-attachment-purchases";
import {procurementAttachmentPurchaseScope,procurementAttachmentFailure,procurementAttachmentSourceReadLocks,type ProcurementAttachmentPurchase} from "@erpx/server-procurement/public/attachment-purchases";
import {financeAttachmentSourceReadLocks} from "@erpx/server-finance/public/attachment-purchases";
import {countAttachmentPurchaseScope,countAttachmentSourceReadLocks,countAttachmentFailure,type CountAttachmentPurchase} from "@erpx/server-inventory/public/count-attachment-purchases";
import {commerceAttachmentPurchaseScope,commerceAttachmentFailure,type CommerceAttachmentPurchase} from "@erpx/server-commerce/public/attachment-purchases";
import {assertExpenseAttachmentPurchases,payrollAttachmentPurchaseScope,type PayrollAttachmentPurchase} from "@erpx/server-people_payroll/public/attachment-purchases";
import {financeAttachmentPurchaseScopes,type FinanceAttachmentPurchase} from "@erpx/server-finance/public/attachment-purchases";
import { supportAttachmentPurchaseScope, type SupportAttachmentPurchase } from "@erpx/server-support/public/attachment-purchases";
import { hospitalityAttachmentPurchaseScope, attachmentPurchaseBoundary, type HospitalityAttachmentPurchase } from "@erpx/server-files/adapters/attachments/hospitality-purchases";
import { purchaseCommandFailure } from "@erpx/server-commercial/public/purchase-command-boundary";
import{createNativeSupportEvidencePlanner}from '@erpx/server-support/public/support-evidence-planning';
import{readSourceRecord,visibleSourceRecord,sourceRecordGuards}from '@erpx/server-records/public/source-record-policy';
const{supportEvidenceGuard,supportEvidenceEvent}=createNativeSupportEvidencePlanner({read:readSourceRecord,visible:visibleSourceRecord,guards:sourceRecordGuards});
import "server-only";
import { createHash, randomUUID } from "node:crypto";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { executeCommand, replayCommand } from "@erpx/server-workflow/public/commands";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { FileStore, SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { isTransactionalSqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { attachmentUploadsFrozen, attachmentsV2Enabled, attachmentsV2ReadOnly, requireAttachmentsV2, requireAttachmentsV2Read } from "@erpx/server-files/public/attachments/feature";
import { attachmentPreviewReadiness, detectedAttachmentMediaType, sanitizedAttachmentPreview } from "@erpx/server-files/public/attachments/preview";
import { portalCollaborationAttachmentRecordVersionStep, portalCollaborationAttachmentTargetVersionGuard } from "@erpx/server-files/public/attachments/portal-collaboration-policy";
import { attachmentScannerReadiness, scanAttachmentBytes } from "@erpx/server-files/public/attachments/scanner";
import { sensitiveAttachmentRecordVersionStep, sensitiveAttachmentTargetVersionGuard } from "@erpx/server-files/public/attachments/sensitive-policy";
import { financeOperationsAttachmentRecordVersionStep, financeOperationsAttachmentTargetVersionGuard } from "@erpx/server-files/public/attachments/finance-operations-policy";
import { remainingOperationalAttachmentRecordVersionStep, remainingOperationalAttachmentTargetVersionGuard } from "@erpx/server-files/public/attachments/remaining-operational-policy";
import type { AttachmentBindingContract } from "@erpx/server-files/public/attachments/types";
import type { AttachmentComponentKind } from "@erpx/server-files/public/attachments/types";
import { createAttachmentService, type AttachmentServiceUnit } from "@erpx/server-files/application/attachments/service";
import type { AttachmentByteStore } from "@erpx/server-files/domain/file-object";
import { repository } from "@erpx/server-files/adapters/attachments/service-repository";
import { attachmentServiceOperationsFacts } from "@erpx/server-operations/public/attachments-service-facts";
import { attachmentServicePeoplePayrollFacts } from "@erpx/server-people_payroll/public/attachments-service-facts";
import { attachmentServiceCommerceFacts } from "@erpx/server-commerce/public/attachments-service-facts";
import { attachmentServiceLegalFacts } from "@erpx/server-legal/public/attachments-service-facts";
import { attachmentServicePlanningFacts } from "@erpx/server-planning/public/attachments-service-facts";
import { attachmentServiceQualityFacts } from "@erpx/server-quality/public/attachments-service-facts";
import { attachmentServiceInventoryFacts } from "@erpx/server-inventory/public/attachments-service-facts";
import { attachmentServiceCollaborationFacts } from "@erpx/server-collaboration/public/attachments-service-facts";
import { attachmentServiceCrmFacts } from "@erpx/server-crm/public/attachments-service-facts";
import { attachmentServiceProjectsFacts } from "@erpx/server-projects/public/attachments-service-facts";
import { attachmentServiceMaintenanceFacts } from "@erpx/server-maintenance/public/attachments-service-facts";
import { attachmentServiceRecordsFacts } from "@erpx/server-records/public/attachments-service-facts";
import { attachmentServiceFinanceFacts } from "@erpx/server-finance/public/attachments-service-facts";
import { attachmentServiceProcurementFacts } from "@erpx/server-procurement/public/attachments-service-facts";
import { attachmentServiceMarketingFacts } from "@erpx/server-marketing/public/attachments-service-facts";
import { attachmentServiceOrganizationFacts } from "@erpx/server-organization/public/attachments-service-facts";
import { attachmentServiceAssetsFacts } from "@erpx/server-assets/public/attachments-service-facts";
import { attachmentServiceSupportFacts } from "@erpx/server-support/public/attachments-service-facts";
import { attachmentServiceFieldServiceFacts } from "@erpx/server-field_service/public/attachments-service-facts";
import { attachmentServiceManufacturingFacts } from "@erpx/server-manufacturing/public/attachments-service-facts";
import { attachmentServiceIdentityAccessFacts } from "@erpx/server-identity_access/public/attachments-service-facts";
import { attachmentServiceManageErpxFacts } from "@erpx/server-manage_erpx/public/attachments-service-facts";
import { attachmentServiceBrandFacts } from "@erpx/server-brand/public/attachments-service-facts";
const nativeDatabases = new WeakMap<AttachmentServiceUnit<SqlStatement>, SqlDatabase>();
const expenseScopesByUnit = new WeakMap<AttachmentServiceUnit<SqlStatement>, readonly PayrollAttachmentPurchase[]>();
type AttachmentRequestBoundary = {
    entered: boolean;
    sourceLocks(database: SqlDatabase): SqlStatement[];
    assertCurrent(database: SqlDatabase): Promise<void>;
    projectResult?(database: SqlDatabase): Promise<void>;
};
function unit(db: SqlDatabase, inheritedPurchases: readonly SupportAttachmentPurchase[] = [], inheritedHospitality: readonly HospitalityAttachmentPurchase[] = [], inheritedFinance:readonly FinanceAttachmentPurchase[] = [], inheritedCommerce:readonly CommerceAttachmentPurchase[] = [], inheritedPayroll:readonly PayrollAttachmentPurchase[] = [], inheritedCounts:readonly CountAttachmentPurchase[] = [],inheritedProcurement:readonly ProcurementAttachmentPurchase[] = [],inheritedPeople:readonly PeopleAttachmentPurchase[] = [],inheritedTransfers:readonly TransferAttachmentPurchase[] = [],inheritedProjects:readonly ProjectAttachmentPurchase[] = [],inheritedQuality:readonly QualityAttachmentPurchase[] = [],inheritedLearning:readonly LearningAttachmentPurchase[] = [],inheritedAssets:readonly AssetAttachmentPurchase[] = [],inheritedLandedCosts:readonly LandedCostAttachmentPurchase[] = [],inheritedTalent:readonly TalentAttachmentPurchase[] = [],inheritedManufacturing:readonly ManufacturingAttachmentPurchase[] = [],inheritedLegal:readonly LegalAttachmentPurchase[] = [],inheritedCRM:readonly CRMAttachmentPurchase[] = [],inheritedMarketing:readonly MarketingAttachmentPurchase[] = [],inheritedWorkforce:readonly WorkforceAttachmentPurchase[] = [],inheritedBenefits:readonly BenefitAttachmentPurchase[] = [],inheritedService:readonly ServiceAttachmentPurchase[] = [],inheritedPortal:readonly PortalAttachmentPurchase[] = [],inheritedWorkTasks:readonly WorkTaskAttachmentPurchase[] = [],inheritedEcommerce:readonly EcommerceAttachmentPurchase[] = [],inheritedMessaging:readonly MessagingAttachmentPurchase[] = [],requestBoundary?:AttachmentRequestBoundary): AttachmentServiceUnit<SqlStatement> {
    const purchaseScopes = [...inheritedPurchases];
    const hospitalityScopes = [...inheritedHospitality];
    const financeScopes=[...inheritedFinance];
    const commerceScopes=[...inheritedCommerce],payrollScopes=[...inheritedPayroll],countScopes=[...inheritedCounts],procurementScopes=[...inheritedProcurement],peopleScopes=[...inheritedPeople],transferScopes=[...inheritedTransfers],projectScopes=[...inheritedProjects],qualityScopes=[...inheritedQuality],learningScopes=[...inheritedLearning],assetScopes=[...inheritedAssets],landedCostScopes=[...inheritedLandedCosts],talentScopes=[...inheritedTalent],manufacturingScopes=[...inheritedManufacturing],legalScopes=[...inheritedLegal],crmScopes=[...inheritedCRM],marketingScopes=[...inheritedMarketing],workforceScopes=[...inheritedWorkforce],benefitScopes=[...inheritedBenefits],serviceScopes=[...inheritedService],portalScopes=[...inheritedPortal],workTaskScopes=[...inheritedWorkTasks],ecommerceScopes=[...inheritedEcommerce],messagingScopes=[...inheritedMessaging];
    const context: AttachmentServiceUnit<SqlStatement> = { ownerVersionGuard: (...args) => { const [commandId, targetType, targetId, actor, version, index] = args, companyId = actor.companyId; const special = portalCollaborationAttachmentTargetVersionGuard(db, commandId, targetType, targetId, companyId, version, index) ?? sensitiveAttachmentTargetVersionGuard(db, commandId, targetType, targetId, companyId, version, index) ?? financeOperationsAttachmentTargetVersionGuard(db, commandId, targetType, targetId, companyId, version, index) ?? remainingOperationalAttachmentTargetVersionGuard(db, commandId, targetType, targetId, companyId, version, index); if (special)
            return special; const effect = attachmentServiceOperationsFacts(db).versionGuard(...args) ?? attachmentServicePeoplePayrollFacts(db).versionGuard(...args) ?? attachmentServiceCommerceFacts(db).versionGuard(...args) ?? attachmentServiceLegalFacts(db).versionGuard(...args) ?? attachmentServicePlanningFacts(db).versionGuard(...args) ?? attachmentServiceQualityFacts(db).versionGuard(...args) ?? attachmentServiceInventoryFacts(db).versionGuard(...args) ?? attachmentServiceCollaborationFacts(db).versionGuard(...args) ?? attachmentServiceCrmFacts(db).versionGuard(...args) ?? attachmentServiceProjectsFacts(db).versionGuard(...args) ?? attachmentServiceMaintenanceFacts(db).versionGuard(...args) ?? attachmentServiceRecordsFacts(db).versionGuard(...args) ?? attachmentServiceFinanceFacts(db).versionGuard(...args) ?? attachmentServiceProcurementFacts(db).versionGuard(...args) ?? attachmentServiceMarketingFacts(db).versionGuard(...args) ?? attachmentServiceOrganizationFacts(db).versionGuard(...args) ?? attachmentServiceAssetsFacts(db).versionGuard(...args) ?? attachmentServiceSupportFacts(db).versionGuard(...args) ?? attachmentServiceFieldServiceFacts(db).versionGuard(...args) ?? attachmentServiceManufacturingFacts(db).versionGuard(...args); if (!effect)
            throw new ApiError(422, "ATTACHMENT_TARGET_UNSUPPORTED", "This file target has no atomic version guard."); return effect; },
        ownerVersionStep: (targetType,targetId,expectedVersion,now,versionContext) => { const args = [targetType,targetId,expectedVersion,now] as const; const serviceStep=fieldServiceAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now,versionContext); if(serviceStep)return serviceStep; const special = portalCollaborationAttachmentRecordVersionStep(db, ...args) ?? sensitiveAttachmentRecordVersionStep(db, ...args) ?? financeOperationsAttachmentRecordVersionStep(db, ...args) ?? remainingOperationalAttachmentRecordVersionStep(db, ...args); if (special)
            return special; const effect = attachmentServiceOperationsFacts(db).versionStep(...args) ?? attachmentServicePeoplePayrollFacts(db).versionStep(...args) ?? attachmentServiceCommerceFacts(db).versionStep(...args) ?? attachmentServiceLegalFacts(db).versionStep(...args) ?? attachmentServicePlanningFacts(db).versionStep(...args) ?? attachmentServiceQualityFacts(db).versionStep(...args) ?? attachmentServiceInventoryFacts(db).versionStep(...args) ?? attachmentServiceCollaborationFacts(db).versionStep(...args) ?? attachmentServiceCrmFacts(db).versionStep(...args) ?? attachmentServiceProjectsFacts(db).versionStep(...args) ?? attachmentServiceMaintenanceFacts(db).versionStep(...args) ?? attachmentServiceRecordsFacts(db).versionStep(...args) ?? attachmentServiceFinanceFacts(db).versionStep(...args) ?? attachmentServiceProcurementFacts(db).versionStep(...args) ?? attachmentServiceMarketingFacts(db).versionStep(...args) ?? attachmentServiceOrganizationFacts(db).versionStep(...args) ?? attachmentServiceAssetsFacts(db).versionStep(...args) ?? attachmentServiceSupportFacts(db).versionStep(...args) ?? attachmentServiceFieldServiceFacts(db).versionStep(...args) ?? attachmentServiceManufacturingFacts(db).versionStep(...args); if (!effect)
            throw new ApiError(422, "RECORD_ATTACHMENT_BIND_UNSUPPORTED", "This record does not support standalone document binding."); return effect; },
        readAttachmentSetAlias: facts => repository(db).readAttachmentSetAlias(facts),
        stepAttachmentSetAlias: facts => repository(db).stepAttachmentSetAlias(facts),
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
        readPublicationRevocation: facts => repository(db).readPublicationRevocation(facts),
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
        atomic: action => isTransactionalSqlDatabase(db) ? db.transaction(async tx => {
            // This per-call boundary belongs only to the complete native request
            // unit. Cleanup is a second original atomic unit and must remain
            // possible after request expiry; it never gains request authority.
            const request = requestBoundary && !requestBoundary.entered ? requestBoundary : undefined;
            if (request) { request.entered = true; await tx.batch([...request.sourceLocks(tx),attachmentPrivateReadSourceLocks(tx)]); }
            await tx.batch(await financeAttachmentSourceReadLocks(tx,financeScopes));await tx.batch(await procurementAttachmentSourceReadLocks(tx,procurementScopes));await tx.batch(await peopleAttachmentSourceReadLocks(tx,peopleScopes));await tx.batch(await projectAttachmentSourceReadLocks(tx,projectScopes));await tx.batch(await countAttachmentSourceReadLocks(tx,countScopes));await tx.batch(await qualityAttachmentSourceReadLocks(tx,qualityScopes));await tx.batch(await learningAttachmentSourceReadLocks(tx,learningScopes));await tx.batch(await assetAttachmentSourceReadLocks(tx,assetScopes));await tx.batch(await landedCostAttachmentSourceReadLocks(tx,landedCostScopes));await tx.batch(await talentAttachmentSourceReadLocks(tx,talentScopes));await tx.batch(await manufacturingAttachmentSourceReadLocks(tx,manufacturingScopes));await tx.batch(await legalAttachmentSourceReadLocks(tx,legalScopes));await tx.batch(await crmAttachmentSourceReadLocks(tx,crmScopes));await tx.batch(await marketingAttachmentSourceReadLocks(tx,marketingScopes));await tx.batch(await workforceAttachmentSourceReadLocks(tx,workforceScopes));await tx.batch(await benefitAttachmentSourceReadLocks(tx,benefitScopes));await tx.batch(await serviceAttachmentSourceReadLocks(tx,serviceScopes));await tx.batch(await portalAttachmentSourceReadLocks(tx,portalScopes));await tx.batch(await workTaskAttachmentSourceReadLocks(tx,workTaskScopes));await tx.batch(await ecommerceAttachmentSourceReadLocks(tx,ecommerceScopes));await tx.batch(await messagingAttachmentSourceReadLocks(tx,messagingScopes));if (request) await request.assertCurrent(tx);
            const ownedUnit = unit(tx, purchaseScopes, hospitalityScopes,financeScopes,commerceScopes,payrollScopes,countScopes,procurementScopes,peopleScopes,transferScopes,projectScopes,qualityScopes,learningScopes,assetScopes,landedCostScopes,talentScopes,manufacturingScopes,legalScopes,crmScopes,marketingScopes,workforceScopes,benefitScopes,serviceScopes,portalScopes,workTaskScopes,ecommerceScopes,messagingScopes);
            const result = await action(ownedUnit);
            if (request) { await flushFinanceDeferredEffects(tx); await request.assertCurrent(tx); if(request.projectResult){await request.projectResult(tx);await request.assertCurrent(tx);} }
            const expenseScopes = expenseScopesByUnit.get(ownedUnit) ?? [];
            if (expenseScopes.some(scope => scope.capability === "people.expenses")) { await flushFinanceDeferredEffects(tx); await assertExpenseAttachmentPurchases(tx, expenseScopes); }
            return result;
        }) : action(context),
        uploadCleanupLock: key => db.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(key).run(), uploadObjectReferenced: (companyId, key) => db.prepare("SELECT 1 FROM attachments WHERE company_id=? AND object_key=?").bind(companyId, key).first(), supportEvidenceEvent: (...args) => supportEvidenceEvent(db, ...args), batch: async effects => { const boundary = await attachmentPurchaseBoundary(db,purchaseScopes,hospitalityScopes,financeScopes,commerceScopes,payrollScopes,countScopes,procurementScopes,peopleScopes,transferScopes,projectScopes,qualityScopes,learningScopes,assetScopes,landedCostScopes,talentScopes,manufacturingScopes,legalScopes,crmScopes,marketingScopes,workforceScopes,benefitScopes,serviceScopes,portalScopes,workTaskScopes,ecommerceScopes,messagingScopes); try { const results = await db.batch([...(boundary?.before ?? []), ...effects, ...(boundary?.after ?? [])]); boundary?.observe?.(results); return results; } catch (error) { throw messagingAttachmentFailure(error) ?? ecommerceAttachmentFailure(error) ?? workTaskAttachmentFailure(error) ?? portalAttachmentFailure(error) ?? serviceAttachmentFailure(error) ?? benefitAttachmentFailure(error) ?? workforceAttachmentFailure(error) ?? crmAttachmentFailure(error) ?? marketingAttachmentFailure(error) ?? legalAttachmentFailure(error) ?? manufacturingAttachmentFailure(error) ?? talentAttachmentFailure(error) ?? landedCostAttachmentFailure(error) ?? assetAttachmentFailure(error) ?? learningAttachmentFailure(error) ?? qualityAttachmentFailure(error) ?? projectAttachmentFailure(error) ?? transferAttachmentFailure(error) ?? procurementAttachmentFailure(error) ?? countAttachmentFailure(error) ?? commerceAttachmentFailure(error) ?? purchaseCommandFailure(error) ?? error; } },
        authorizeTarget: async (...args) => { const {authorizeAttachmentTarget}=await import("@erpx/server-files/adapters/attachments/policy"); const scope = await authorizeAttachmentTarget(db, ...args); if (args[3] !== "read") { const purchase = supportAttachmentPurchaseScope(args[0], args[1].targetType); if (purchase) purchaseScopes.push(purchase); const hospitality = hospitalityAttachmentPurchaseScope(args[0], args[1]); if (hospitality) hospitalityScopes.push(hospitality); financeScopes.push(...await financeAttachmentPurchaseScopes(db,args[0],args[1].targetType,args[2]));const order=await commerceAttachmentPurchaseScope(db,args[0],args[1],args[2]);if(order)commerceScopes.push(order);const payroll=await payrollAttachmentPurchaseScope(db,args[0],args[1].targetType,args[2]);if(payroll)payrollScopes.push(payroll);const count=await countAttachmentPurchaseScope(db,args[0],args[1].targetType,args[2]);if(count)countScopes.push(count);const rfq=await procurementAttachmentPurchaseScope(db,args[0],args[1],args[2]);if(rfq)procurementScopes.push(rfq);const person=await peopleAttachmentPurchaseScope(db,args[0],args[1],args[2]);if(person)peopleScopes.push(person);const transfer=await transferAttachmentPurchaseScope(db,args[0],args[1],args[2]);if(transfer)transferScopes.push(transfer);const project=await projectAttachmentPurchaseScope(db,args[0],args[1].targetType,args[2]);if(project)projectScopes.push(project);const quality=await qualityAttachmentPurchaseScope(db,args[0],args[1],args[2]);if(quality)qualityScopes.push(quality);const learning=await learningAttachmentPurchaseScope(db,args[0],args[1].targetType,args[2]);if(learning)learningScopes.push(learning);const asset=await assetAttachmentPurchaseScope(db,args[0],args[1],args[2]);if(asset)assetScopes.push(asset);const landed=await landedCostAttachmentPurchaseScope(db,args[0],args[1].targetType,args[2]);if(landed)landedCostScopes.push(landed);const talent=await talentAttachmentPurchaseScope(db,args[0],args[1],args[2]);if(talent)talentScopes.push(talent);const production=await manufacturingAttachmentPurchaseScope(db,args[0],args[1].targetType,args[2]);if(production)manufacturingScopes.push(production);const legal=await legalAttachmentPurchaseScope(db,args[0],args[1].targetType,args[2]);if(legal)legalScopes.push(legal);const crm=await crmAttachmentPurchaseScope(db,args[0],args[1],args[2]);if(crm)crmScopes.push(crm);const marketing=await marketingAttachmentPurchaseScope(db,args[0],args[1],args[2],args[3]);if(marketing)marketingScopes.push(marketing);const workforce=await workforceAttachmentPurchaseScope(db,args[0],args[1],args[2]);if(workforce)workforceScopes.push(workforce);const benefit=await benefitAttachmentPurchaseScope(db,args[0],args[1],args[2]);if(benefit)benefitScopes.push(benefit);const service=await serviceAttachmentPurchaseScope(db,args[0],args[1],args[2]);if(service)serviceScopes.push(service);const portal=await portalAttachmentPurchaseScope(db,args[0],args[1],args[2],args[3]);if(portal)portalScopes.push(portal);const workTask=await workTaskAttachmentPurchaseScope(db,args[0],args[1].targetType,args[2],args[3]);if(workTask)workTaskScopes.push(workTask);const ecommerce=await ecommerceAttachmentPurchaseScope(db,args[0],args[1],args[2],args[3]);if(ecommerce)ecommerceScopes.push(ecommerce);const messaging=await messagingAttachmentPurchaseScope(db,args[0],args[1].targetType,args[2],args[3]);if(messaging)messagingScopes.push(messaging); } return scope; },
        supportEvidenceGuard: async (...args) => await supportEvidenceGuard(db, ...args),
        replayCommand: (...args) => replayCommand(db, ...args),
        executeCommand: async (...args) => { const boundary = await attachmentPurchaseBoundary(db,purchaseScopes,hospitalityScopes,financeScopes,commerceScopes,payrollScopes,countScopes,procurementScopes,peopleScopes,transferScopes,projectScopes,qualityScopes,learningScopes,assetScopes,landedCostScopes,talentScopes,manufacturingScopes,legalScopes,crmScopes,marketingScopes,workforceScopes,benefitScopes,serviceScopes,portalScopes,workTaskScopes,ecommerceScopes,messagingScopes); try { return await executeCommand(db, args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7], args[8], boundary); } catch (error) { throw messagingAttachmentFailure(error) ?? ecommerceAttachmentFailure(error) ?? workTaskAttachmentFailure(error) ?? portalAttachmentFailure(error) ?? serviceAttachmentFailure(error) ?? benefitAttachmentFailure(error) ?? workforceAttachmentFailure(error) ?? crmAttachmentFailure(error) ?? marketingAttachmentFailure(error) ?? legalAttachmentFailure(error) ?? manufacturingAttachmentFailure(error) ?? talentAttachmentFailure(error) ?? landedCostAttachmentFailure(error) ?? assetAttachmentFailure(error) ?? learningAttachmentFailure(error) ?? qualityAttachmentFailure(error) ?? projectAttachmentFailure(error) ?? transferAttachmentFailure(error) ?? procurementAttachmentFailure(error) ?? countAttachmentFailure(error) ?? commerceAttachmentFailure(error) ?? purchaseCommandFailure(error) ?? error; } } };
    expenseScopesByUnit.set(context, payrollScopes);
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
type PortalAttachmentMetadataCommand =
    | {kind:'create';input:{targetType:'customer_access_binding';targetId:string;componentKind:'action';componentRef?:string;purposeKey:'customer_support_evidence';expectedTargetVersion:number};key:string}
    | {kind:'reserve';setId:string;input:{filename:string;mediaType:string;bytes:number;caption?:string};key:string}
    | {kind:'withdraw';linkId:string;reason:string;key:string};
type PortalAttachmentMetadataRead =
    | {kind:'sets';input:{targetType:'customer_access_binding';targetId:string;purposeKey:'customer_support_evidence';componentKind:'action';expectedTargetVersion?:number}}
    | {kind:'status';linkId:string};
/** Closed Portal metadata reads keep their original read policies and DTOs.
 * The final request assertion belongs to this fresh native Holding unit. */
export async function portalAttachmentMetadataReadForRequest(db:SqlDatabase,actor:WorkContext,read:PortalAttachmentMetadataRead,request:Pick<AttachmentRequestBoundary,'sourceLocks'|'assertCurrent'>){
    if(!isTransactionalSqlDatabase(db))throw new ApiError(503,'ATTACHMENT_REQUEST_TRANSACTION_REQUIRED','A file request requires its complete fresh native transaction.');
    return db.transaction(async tx=>{
        await tx.batch([...request.sourceLocks(tx),attachmentPrivateReadSourceLocks(tx)]);
        await request.assertCurrent(tx);
        const result=read.kind==='sets'?await service.listAttachmentSets(unit(tx),actor,read.input):await service.attachmentLinkStatus(unit(tx),actor,read.linkId);
        await flushFinanceDeferredEffects(tx);await request.assertCurrent(tx);
        return result;
    });
}
/** The three existing Portal staging commands retain their original receipts
 * and transaction; the status returned after withdrawal is read before commit. */
export async function portalAttachmentCommandForRequest(db:SqlDatabase,actor:WorkContext,command:PortalAttachmentMetadataCommand,request:Pick<AttachmentRequestBoundary,'sourceLocks'|'assertCurrent'>){
    if(!isTransactionalSqlDatabase(db))throw new ApiError(503,'ATTACHMENT_REQUEST_TRANSACTION_REQUIRED','A file request requires its complete fresh native transaction.');
    let status:Awaited<ReturnType<typeof service.attachmentLinkStatus>>|undefined;
    const boundary:AttachmentRequestBoundary={...request,entered:false,...(command.kind==='withdraw'?{projectResult:async(database:SqlDatabase)=>{status=await service.attachmentLinkStatus(unit(database),actor,command.linkId);}}:{})};
    const context=unit(db, [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], boundary);
    if(command.kind==='create')return service.createAttachmentSet(context,actor,command.input,command.key);
    if(command.kind==='reserve')return service.reserveAttachmentFile(context,actor,command.setId,command.input,command.key);
    await service.withdrawAttachmentLink(context,actor,command.linkId,command.reason,command.key);
    if(!status)throw new Error('PORTAL_WITHDRAWAL_RESULT_REQUIRED');
    return status;
}
/** A request's final assertion runs in the original metadata transaction,
 * so the application's existing object compensation still catches failure.
 * There is no authority token or fallible check after a write has committed. */
export async function completeAttachmentUploadForRequest(db: SqlDatabase, files: FileStore, actor: WorkContext, uploadIntentId: string, buffer: ArrayBuffer, request: Pick<AttachmentRequestBoundary, 'sourceLocks' | 'assertCurrent'>) {
    if (!isTransactionalSqlDatabase(db)) throw new ApiError(503, 'ATTACHMENT_REQUEST_TRANSACTION_REQUIRED', 'A request upload requires its complete fresh native transaction.');
    const boundary: AttachmentRequestBoundary = { ...request, entered: false };
    const context = unit(db, [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], [], boundary);
    const result = await service.completeAttachmentUpload(context, byteStore(files), actor, uploadIntentId, buffer);
    // The original identical uploaded-byte replay bypasses atomic. Repeat its
    // exact original reader under fresh source leases; require the persisted
    // completed status before re-entering it, so this unit cannot start a new
    // object write or a metadata command outside the application's catch.
    if (!boundary.entered) return db.transaction(async tx => {
        await tx.batch([...boundary.sourceLocks(tx), attachmentPrivateReadSourceLocks(tx)]);
        await boundary.assertCurrent(tx);
        const intent = await repository(tx).readUploadIntents28({uploadIntentId,userId:actor.userId,companyId:actor.companyId});
        if (!intent || !['uploaded','ready'].includes(String(intent.status))) throw new ApiError(409,'UPLOAD_CONFLICT','The completed upload changed. Reload before retrying.');
        const replay = await service.completeAttachmentUpload(unit(tx), byteStore(files), actor, uploadIntentId, buffer);
        await flushFinanceDeferredEffects(tx); await boundary.assertCurrent(tx);
        return replay;
    });
    return result;
}
export function attachmentLinkStatus(db: SqlDatabase, actor: WorkContext, linkId: string) { return service.attachmentLinkStatus(unit(db), actor, linkId); }
export async function portalAttachmentContentForRequest(db: SqlDatabase, files: FileStore, actor: WorkContext, linkId: string, assertRequestAuthority: () => Promise<void>) {
    const selected = await attachmentReadRoute(db, actor.companyId, linkId);
    if (selected?.targetType !== 'customer_access_binding') throw new ApiError(404, 'ATTACHMENT_LINK_NOT_FOUND', 'File link not found.');
    const authorize = async (tx: SqlDatabase, source: Pick<AttachmentReadRoute, 'targetType' | 'targetId' | 'purposeKey' | 'componentKind'>) => {
        const policy = await import('@erpx/server-files/adapters/attachments/policy');
        return policy.authorizeAttachmentTarget(tx, actor, policy.attachmentPolicy(source.targetType, source.purposeKey, source.componentKind), source.targetId, 'read');
    };
    return withPortalAttachmentContentRead(db, actor, linkId, selected, tx => service.attachmentContent(unit(tx), byteStore(files), actor, linkId, 'preview'), authorize, assertRequestAuthority);
}
export async function attachmentContent(db: SqlDatabase, files: FileStore, actor: WorkContext, linkId: string, variant: "original" | "preview", options: AttachmentServiceOptions = {}) {
    const selected=await attachmentReadRoute(db,actor.companyId,linkId),read=(tx:SqlDatabase)=>service.attachmentContent(unit(tx),byteStore(files),actor,linkId,variant,options);
    const authorize=async(tx:SqlDatabase,source:Pick<AttachmentReadRoute,'targetType'|'targetId'|'purposeKey'|'componentKind'>)=>{const policy=await import('@erpx/server-files/adapters/attachments/policy');return policy.authorizeAttachmentTarget(tx,actor,policy.attachmentPolicy(source.targetType,source.purposeKey,source.componentKind),source.targetId,'read');};
    if(selected?.targetType==='staff_task')return withWorkTaskAttachmentContentRead(db,actor,linkId,selected,read,async(tx,source)=>authorize(tx,{...source,targetType:'staff_task'}));
    if(selected?.targetType==='customer_access_binding')return withPortalAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected?.targetType==='ecommerce_listing')return withEcommerceAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected?.targetType==='finance_tax_identity')return withFinanceTaxSignatureAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected?.targetType==='messaging_room')return withMessagingAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected?.targetType==='legal_item')return withLegalCanonicalAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&hasLegalEvidenceAttachmentReadParent(db))return withLegalEvidenceAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isSocialAttachmentReadTarget(selected.targetType))return withSocialAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isEmployeeEvidenceAttachmentType(selected.targetType))return withEmployeeEvidenceAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected?.targetType==='supplier_bill')return withSupplierBillAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isFinanceAttachmentReadTarget(selected.targetType))return withFinanceAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isHospitalityAttachmentReadTarget(selected.targetType))return withHospitalityAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected?.targetType==='support_incident')return withSupportIncidentAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected?.targetType==='support_ticket')return withSupportAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected?.targetType==='field_service_visit')return withServiceAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isPeopleAttachmentReadTarget(selected.targetType))return withPeopleAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isProjectAttachmentReadTarget(selected.targetType))return withProjectAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isLearningAttachmentReadTarget(selected.targetType))return withLearningAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isCRMAttachmentReadTarget(selected.targetType))return withCRMAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isMarketingAttachmentReadTarget(selected.targetType))return withMarketingAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isAssetAttachmentReadTarget(selected.targetType))return withAssetAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isInventoryAttachmentReadTarget(selected.targetType))return withInventoryAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isManufacturingAttachmentReadTarget(selected.targetType))return withManufacturingAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isCommerceAttachmentReadTarget(selected.targetType))return withCommerceAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isProcurementAttachmentReadTarget(selected.targetType))return withProcurementAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isBenefitAttachmentReadTarget(selected.targetType))return withBenefitAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isQualityAttachmentReadTarget(selected.targetType))return withQualityAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isTalentAttachmentReadTarget(selected.targetType))return withTalentAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isBrandAttachmentReadTarget(selected.targetType))return withBrandAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isOrganizationAttachmentReadTarget(selected.targetType))return withOrganizationAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isIamAttachmentReadTarget(selected.targetType))return withIamAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isCompanyReleaseAttachmentReadTarget(selected.targetType))return withCompanyReleaseAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    if(selected&&isCompanyProvisioningAttachmentReadTarget(selected.targetType))return withCompanyProvisioningAttachmentContentRead(db,actor,linkId,selected,read,authorize);
    // Other canonical owners still compose with their original Holding unit.
    // Their complete parent boundary must finish before a shared read unit can
    // flush deferred Finance or Legal effects.
    return read(db);
}
export function withdrawAttachmentLink(db: SqlDatabase, actor: WorkContext, linkId: string, reason: string, idempotencyKey: string,options?:{compatibilityAdapter?:boolean}) { return options?service.withdrawAttachmentLink(unit(db), actor, linkId, reason, idempotencyKey,options):service.withdrawAttachmentLink(unit(db), actor, linkId, reason, idempotencyKey); }
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
export function attachmentRecordVersionStep(db: SqlDatabase, targetType: string, targetId: string, expectedVersion: number, now = Date.now(), context?: {actor:WorkContext;commandId:string}) { return service.attachmentRecordVersionStep(unit(db), targetType, targetId, expectedVersion, now, context); }
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
export function publicAttachmentPublication(db: SqlDatabase, files: FileStore, id: string) { return withPublicationContentRead(db,id,database=>service.publicAttachmentPublication(unit(database),byteStore(files),id)); }
export function drainAttachmentScans(db: SqlDatabase, files: FileStore, attachmentId?: string, options: {
    offlineMigration?: boolean;
} = {}) { return service.drainAttachmentScans(unit(db), byteStore(files), attachmentId, options); }
export function scanAttachmentNow(db: SqlDatabase, files: FileStore, attachmentId: string) { return service.scanAttachmentNow(unit(db), byteStore(files), attachmentId); }
export function attachmentDestructionEligibility(db: SqlDatabase, attachmentId: string, now = Date.now()) { return service.attachmentDestructionEligibility(unit(db), attachmentId, now); }
export function cleanupAttachmentDrafts(db: SqlDatabase, files: FileStore) { return service.cleanupAttachmentDrafts(unit(db), byteStore(files)); }
