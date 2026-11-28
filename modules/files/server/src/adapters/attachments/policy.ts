import{createNativeSupportTicketRead}from '@erpx/support-server/public/support-ticket-read';
import{supportEvidenceGuard}from '@erpx/support-server/public/support-evidence';
import{readSourceRecord,visibleSourceRecord,sourceRecordGuards}from '@erpx/records-server/public/source-record-policy';
const sources={read:readSourceRecord,visible:visibleSourceRecord,guards:sourceRecordGuards};
const supportTicketRead=createNativeSupportTicketRead(sources);
import {authorizeMessagingAttachmentPurchase} from "@erpx/collaboration-server/public/messaging-attachment-purchases";
import {authorizeEcommerceAttachmentPurchase} from "@erpx/ecommerce-server/public/attachment-purchases";
import {authorizeWorkTaskAttachmentPurchase} from "@erpx/operations-server/public/task-attachment-purchases";
import {authorizePortalAttachmentPurchase} from "@erpx/customer-authority-server/public/attachment-purchases";
import {authorizeBenefitAttachmentPurchase} from "@erpx/benefits-server/public/attachment-purchases";
import {authorizeWorkforceAttachmentPurchase} from "@erpx/people-payroll-server/public/workforce-attachment-purchases";
import {authorizeCRMAttachmentPurchase} from "@erpx/crm-server/public/attachment-purchases";
import {authorizeMarketingAttachmentPurchase} from "@erpx/marketing-server/public/attachment-purchases";
import {authorizeManufacturingAttachmentPurchase} from "@erpx/manufacturing-server/public/attachment-purchases";
import {authorizeLegalAttachmentPurchase} from "@erpx/legal-server/public/attachment-purchases";
import {authorizeTalentAttachmentPurchase} from "@erpx/talent-server/public/attachment-purchases";
import {authorizeLandedCostAttachmentPurchase} from "@erpx/inventory-server/public/landed-cost-attachment-purchases";
import {authorizeInventoryLotAttachmentRead} from '@erpx/inventory-server/public/lot-attachment-purchases';
import {authorizeCommerceProductAttachmentRead} from '@erpx/commerce-server/public/product-attachment-purchases';
import {authorizeAssetAttachmentPurchase} from "@erpx/assets-server/public/attachment-purchases";
import {authorizeLearningAttachmentPurchase} from "@erpx/learning-server/public/attachment-purchases";
import {authorizeQualityAttachmentPurchase} from "@erpx/quality-server/public/attachment-purchases";
import {authorizeTransferAttachmentPurchase} from "@erpx/inventory-server/public/transfer-attachment-purchases";
import {authorizePeopleAttachmentPurchase} from "@erpx/people-payroll-server/public/base-attachment-purchases";
import {authorizeProcurementAttachmentPurchase} from "@erpx/procurement-server/public/attachment-purchases";
import {authorizeFinanceAttachmentPurchase} from "@erpx/finance-server/public/attachment-purchases";
import {authorizeCommerceAttachmentPurchase} from "@erpx/commerce-server/public/attachment-purchases";
import {authorizePayrollAttachmentPurchase} from "@erpx/people-payroll-server/public/attachment-purchases";
import {authorizeCountAttachmentPurchase} from "@erpx/inventory-server/public/count-attachment-purchases";
import { authorizeHospitalityAttachmentPurchase } from "@erpx/files-server/adapters/attachments/hospitality-purchases";
import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { AttachmentComponentKind, AttachmentOperation, AttachmentPolicy } from "@erpx/files-server/public/attachments/types";
import { authorizePortalCollaborationAttachmentTarget } from "@erpx/files-server/public/attachments/portal-collaboration-policy";
import { authorizeSensitiveAttachmentTarget } from "@erpx/files-server/public/attachments/sensitive-policy";
import { authorizeFinanceOperationsAttachmentTarget } from "@erpx/files-server/public/attachments/finance-operations-policy";
import { authorizeRemainingOperationalAttachmentTarget } from "@erpx/files-server/public/attachments/remaining-operational-policy";
import { createAttachmentPolicy, type AttachmentPolicyUnit } from "@erpx/files-server/application/attachments/policy";
import { policyProjectsFacts } from "@erpx/projects-server/public/attachments-policy-facts";
import { policyFinanceFacts } from "@erpx/finance-server/public/attachments-policy-facts";
import { policyCommerceFacts } from "@erpx/commerce-server/public/attachments-policy-facts";
import { policyQualityFacts } from "@erpx/quality-server/public/attachments-policy-facts";
import { policyProcurementFacts } from "@erpx/procurement-server/public/attachments-policy-facts";
import { policyPlanningFacts } from "@erpx/planning-server/public/attachments-policy-facts";
import { policyMaintenanceFacts } from "@erpx/maintenance-server/public/attachments-policy-facts";
import { policyAssetsFacts } from "@erpx/assets-server/public/attachments-policy-facts";
import { policyManufacturingFacts } from "@erpx/manufacturing-server/public/attachments-policy-facts";
import { policyInventoryFacts } from "@erpx/inventory-server/public/attachments-policy-facts";
import { policyCrmFacts } from "@erpx/crm-server/public/attachments-policy-facts";
import { policyMarketingFacts } from "@erpx/marketing-server/public/attachments-policy-facts";
import { policyPosFacts } from "@erpx/pos-server/public/attachments-policy-facts";
import { policyIdentityAccessFacts } from "@erpx/identity-access-server/public/attachments-policy-facts";
import { policyOrganizationFacts } from "@erpx/organization-server/public/attachments-policy-facts";
function unit(db: SqlDatabase): AttachmentPolicyUnit {
    return { readDescriptor: async (facts) => { switch (facts.targetType) {
            case "procurement_rfq": return policyProcurementFacts(db).readDescriptor(facts);
            case "purchase_order": return policyCommerceFacts(db).readDescriptor(facts);
            case "supplier_bill": return policyFinanceFacts(db).readDescriptor(facts);
            case "landed_cost": return policyInventoryFacts(db).readDescriptor(facts);
            case "stock_count": return policyInventoryFacts(db).readDescriptor(facts);
            case "inventory_stock_count": return policyInventoryFacts(db).readDescriptor(facts);
            case "stock_transfer": return policyInventoryFacts(db).readDescriptor(facts);
            case "inventory_transfer": return policyInventoryFacts(db).readDescriptor(facts);
            case "inventory_transit": return policyInventoryFacts(db).readDescriptor(facts);
            case "product": return policyCommerceFacts(db).readDescriptor(facts);
            case "inventory_lot": return policyInventoryFacts(db).readDescriptor(facts);
            case "quality_hold": return policyQualityFacts(db).readDescriptor(facts);
            case "inventory_quality_hold": return policyQualityFacts(db).readDescriptor(facts);
            case "commerce_order": return policyCommerceFacts(db).readDescriptor(facts);
            case "commerce_order_delivery": return policyCommerceFacts(db).readDescriptor(facts);
            case "commerce_return": return policyCommerceFacts(db).readDescriptor(facts);
            case "asset": return policyAssetsFacts(db).readDescriptor(facts);
            case "maintenance_work": return policyMaintenanceFacts(db).readDescriptor(facts);
            case "production_run": return policyManufacturingFacts(db).readDescriptor(facts);
            case "planning_scenario": return policyPlanningFacts(db).readDescriptor(facts);
            case "project": return policyProjectsFacts(db).readDescriptor(facts);
            default: return null;
        } },
        readCrmAccount: facts => policyCrmFacts(db).readCrmAccount(facts),
        readCrmOpportunity: facts => policyCrmFacts(db).readCrmOpportunity(facts),
        readMarketingCampaign: facts => policyMarketingFacts(db).readMarketingCampaign(facts),
        readPosProduct: facts => policyPosFacts(db).readPosProduct(facts),
        readCurrentAssignment: facts => policyIdentityAccessFacts(db).readCurrentAssignment(facts),
        readSalesDelivery: facts => policyCommerceFacts(db).readSalesDelivery(facts),
        readPurchaseReceipt: facts => policyCommerceFacts(db).readPurchaseReceipt(facts),
        readFulfillment: facts => policyCommerceFacts(db).readFulfillment(facts),
        readPurchaseOrder: facts => policyCommerceFacts(db).readPurchaseOrder(facts),
        readQualityHold: facts => policyQualityFacts(db).readQualityHold(facts),
        readTransit: facts => policyInventoryFacts(db).readTransit(facts),
        readProduct: facts => policyCommerceFacts(db).readProduct(facts),
        readProject: facts => policyProjectsFacts(db).readProject(facts),
        readBranchBrand: facts => policyOrganizationFacts(db).readBranchBrand(facts),
        authorizePortalCollaborationAttachmentTarget: (...args) => authorizePortalCollaborationAttachmentTarget(db, ...args),
        authorizeSensitiveAttachmentTarget: (...args) => authorizeSensitiveAttachmentTarget(db, ...args),
        authorizeFinanceOperationsAttachmentTarget: (...args) => authorizeFinanceOperationsAttachmentTarget(db, ...args),
        authorizeRemainingOperationalAttachmentTarget: (...args) => authorizeRemainingOperationalAttachmentTarget(db, ...args),
        messagingRoom: async (actor, id) => await (await import("@erpx/collaboration-server/public/messaging-read")).messagingRoom(db, actor, id) as unknown as Record<string, unknown>,
        legalAccess: async (actor, id, permission) => await (await import("@erpx/legal-server/public/legal/access")).access(db, actor, id, permission) as unknown as Record<string, unknown>,
        leaveAccess: async (actor, input) => { const service = await import("@erpx/people-payroll-server/public/leave-evidence"); if (!service.assertLeaveAttachmentAccess)
            throw new ApiError(503, "LEAVE_ATTACHMENT_ADAPTER_UNAVAILABLE", "Leave evidence access is not ready."); return service.assertLeaveAttachmentAccess(db, actor, input); },
        supportTicket: async (actor, id) => await supportTicketRead.getSupportTicket(actor, id, db),
        supportEvidenceGuard: async (actor, id, version, key) => await supportEvidenceGuard(db, actor, id, version, key),
        employeeRequest: async (actor, id) => await (await import("@erpx/records-server/public/employee-requests-read")).getEmployeeRequest(db, actor, id),
        employeeRequestUpload: async (actor, id, purpose) => await (await import("@erpx/records-server/public/employee-requests-read")).employeeRequestUpload(db, actor, id, purpose),
        expenseOwner: async (actor, id) => { const service = await import("@erpx/people-payroll-server/public/expense-read"), row = await service.getExpense(actor, id, db); return { row, canEdit: service.canEditExpense(actor, row) }; },
        expensePayment: async (actor, id, editable) => await (await import("@erpx/people-payroll-server/public/expense-payment-read")).paymentDraft(actor, id, editable, db),
        invoicePaymentOwner: async (actor, id, editable) => await (await import("@erpx/finance-server/public/invoice-payment-evidence")).invoicePaymentOwner(db, actor, id, editable),
        taskOwner: async (actor, id) => { const service = await import("@erpx/operations-server/public/staff-task-read"), row = await (await import("@erpx/operations-server/public/staff-task-read")).getTask(db, actor, id); return { row, canAttach: service.canAttachTask(actor, row) }; },
        socialUploadOwner: async (actor, id, editable) => await (await import("@erpx/collaboration-server/public/social-home-read")).socialUploadOwner(db, actor, id, editable),
        requireBrandManagement: async (actor, type, id) => await (await import("@erpx/brand-server/public/branding")).requireBrandManagement(db, actor, type, id) };
}
const service = createAttachmentPolicy({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message) });
export const attachmentPolicyRegistry = service.attachmentPolicyRegistry;
export function attachmentRetentionSnapshot(policy: AttachmentPolicy) { return service.attachmentRetentionSnapshot(policy); }
export function attachmentPolicy(targetType: string, purposeKey: string, componentKind: AttachmentComponentKind) { return service.attachmentPolicy(targetType, purposeKey, componentKind); }
export function authorizePosProductImageTarget(db: SqlDatabase, actor: WorkContext, targetId: string, operation: AttachmentOperation) { return service.authorizePosProductImageTarget(unit(db), actor, targetId, operation); }
export async function authorizeAttachmentTarget(db: SqlDatabase, actor: WorkContext, policy: AttachmentPolicy, targetId: string, operation: AttachmentOperation) { if(policy.targetType==='field_service_visit')throw new ApiError(503,'FIELD_FILES_NATIVE_REQUIRED','The native Field file service is required.'); const scope = await service.authorizeAttachmentTarget(unit(db), actor, policy, targetId, operation); await authorizeInventoryLotAttachmentRead(db,actor,policy.targetType,operation); await authorizeCommerceProductAttachmentRead(db,actor,policy.targetType,operation); await authorizeHospitalityAttachmentPurchase(db, actor, policy, operation); if(["supplier_bill","invoice_payment"].includes(policy.targetType))await authorizeFinanceAttachmentPurchase(db,actor,policy.targetType,targetId,operation);await authorizeCommerceAttachmentPurchase(db,actor,policy,targetId,operation);await authorizePayrollAttachmentPurchase(db,actor,policy.targetType,operation);await authorizeCountAttachmentPurchase(db,actor,policy.targetType,targetId,operation);await authorizeProcurementAttachmentPurchase(db,actor,policy,targetId,operation);await authorizePeopleAttachmentPurchase(db,actor,policy.targetType,operation);await authorizeTransferAttachmentPurchase(db,actor,policy,targetId,operation);await authorizeQualityAttachmentPurchase(db,actor,policy,targetId,operation);await authorizeLearningAttachmentPurchase(db,actor,policy.targetType,targetId,operation);await authorizeAssetAttachmentPurchase(db,actor,policy,targetId,operation);await authorizeLandedCostAttachmentPurchase(db,actor,policy.targetType,targetId,operation);await authorizeTalentAttachmentPurchase(db,actor,policy,targetId,operation);await authorizeManufacturingAttachmentPurchase(db,actor,policy.targetType,targetId,operation);await authorizeLegalAttachmentPurchase(db,actor,policy.targetType,targetId,operation);await authorizeCRMAttachmentPurchase(db,actor,policy,targetId,operation);await authorizeMarketingAttachmentPurchase(db,actor,policy,targetId,operation);await authorizeWorkforceAttachmentPurchase(db,actor,policy,targetId,operation);await authorizeBenefitAttachmentPurchase(db,actor,policy,targetId,operation);await authorizePortalAttachmentPurchase(db,actor,policy,targetId,operation);await authorizeWorkTaskAttachmentPurchase(db,actor,policy.targetType,targetId,operation);await authorizeEcommerceAttachmentPurchase(db,actor,policy,targetId,operation);await authorizeMessagingAttachmentPurchase(db,actor,policy.targetType,targetId,operation); return scope; }
export function assertAttachmentMedia(policy: AttachmentPolicy, mediaType: string, bytes: number) { return service.assertAttachmentMedia(policy, mediaType, bytes); }
