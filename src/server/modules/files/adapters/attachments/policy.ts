import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { ApiError } from "@/server/modules/runtime/public/http";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import type { AttachmentComponentKind, AttachmentOperation, AttachmentPolicy } from "@/server/modules/files/public/attachments/types";
import { authorizePortalCollaborationAttachmentTarget } from "@/server/modules/files/public/attachments/portal-collaboration-policy";
import { authorizeSensitiveAttachmentTarget } from "@/server/modules/files/public/attachments/sensitive-policy";
import { authorizeFinanceOperationsAttachmentTarget } from "@/server/modules/files/public/attachments/finance-operations-policy";
import { authorizeRemainingOperationalAttachmentTarget } from "@/server/modules/files/public/attachments/remaining-operational-policy";
import { createAttachmentPolicy, type AttachmentPolicyUnit } from "../../application/attachments/policy";
import { policyProjectsFacts } from "../../../projects/public/attachments-policy-facts";
import { policyFinanceFacts } from "../../../finance/public/attachments-policy-facts";
import { policyCommerceFacts } from "../../../commerce/public/attachments-policy-facts";
import { policyQualityFacts } from "../../../quality/public/attachments-policy-facts";
import { policyProcurementFacts } from "../../../procurement/public/attachments-policy-facts";
import { policyPlanningFacts } from "../../../planning/public/attachments-policy-facts";
import { policyMaintenanceFacts } from "../../../maintenance/public/attachments-policy-facts";
import { policyAssetsFacts } from "../../../assets/public/attachments-policy-facts";
import { policyFieldServiceFacts } from "../../../field_service/public/attachments-policy-facts";
import { policyManufacturingFacts } from "../../../manufacturing/public/attachments-policy-facts";
import { policyInventoryFacts } from "../../../inventory/public/attachments-policy-facts";
import { policyCrmFacts } from "../../../crm/public/attachments-policy-facts";
import { policyMarketingFacts } from "../../../marketing/public/attachments-policy-facts";
import { policyPosFacts } from "../../../pos/public/attachments-policy-facts";
import { policyIdentityAccessFacts } from "../../../identity_access/public/attachments-policy-facts";
import { policyOrganizationFacts } from "../../../organization/public/attachments-policy-facts";
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
            case "field_service_visit": return policyFieldServiceFacts(db).readDescriptor(facts);
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
        messagingRoom: async (actor, id) => await (await import("../../../collaboration/public/messaging")).messagingRoom(db, actor, id) as unknown as Record<string, unknown>,
        legalAccess: async (actor, id, permission) => await (await import("../../../legal/public/legal/access")).access(db, actor, id, permission) as unknown as Record<string, unknown>,
        leaveAccess: async (actor, input) => { const service = await import("../../../people_payroll/public/leave"); if (!service.assertLeaveAttachmentAccess)
            throw new ApiError(503, "LEAVE_ATTACHMENT_ADAPTER_UNAVAILABLE", "Leave evidence access is not ready."); return service.assertLeaveAttachmentAccess(db, actor, input); },
        supportTicket: async (actor, id) => await (await import("../../../support/public/support")).getSupportTicket(actor, id, db),
        supportEvidenceGuard: async (actor, id, version, key) => await (await import("../../../support/public/support-evidence")).supportEvidenceGuard(db, actor, id, version, key),
        employeeRequest: async (actor, id) => await (await import("../../../records/public/employee-requests")).getEmployeeRequest(db, actor, id),
        employeeRequestUpload: async (actor, id, purpose) => await (await import("../../../records/public/employee-requests")).employeeRequestUpload(db, actor, id, purpose),
        expenseOwner: async (actor, id) => { const service = await import("../../../people_payroll/public/expenses"), row = await service.getExpense(actor, id, db); return { row, canEdit: service.canEditExpense(actor, row) }; },
        expensePayment: async (actor, id, editable) => await (await import("../../../people_payroll/public/expense-payments")).paymentDraft(actor, id, editable, db),
        invoicePaymentOwner: async (actor, id, editable) => await (await import("../../../finance/public/invoice-payment-evidence")).invoicePaymentOwner(db, actor, id, editable),
        taskOwner: async (actor, id) => { const service = await import("../../../operations/public/staff-tasks"), row = await service.getTask(db, actor, id); return { row, canAttach: service.canAttachTask(actor, row) }; },
        socialUploadOwner: async (actor, id, editable) => await (await import("../../../collaboration/public/social-home")).socialUploadOwner(db, actor, id, editable),
        requireBrandManagement: async (actor, type, id) => await (await import("../../../brand/public/branding")).requireBrandManagement(db, actor, type, id) };
}
const service = createAttachmentPolicy({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message) });
export const attachmentPolicyRegistry = service.attachmentPolicyRegistry;
export function attachmentRetentionSnapshot(policy: AttachmentPolicy) { return service.attachmentRetentionSnapshot(policy); }
export function attachmentPolicy(targetType: string, purposeKey: string, componentKind: AttachmentComponentKind) { return service.attachmentPolicy(targetType, purposeKey, componentKind); }
export function authorizePosProductImageTarget(db: SqlDatabase, actor: WorkContext, targetId: string, operation: AttachmentOperation) { return service.authorizePosProductImageTarget(unit(db), actor, targetId, operation); }
export function authorizeAttachmentTarget(db: SqlDatabase, actor: WorkContext, policy: AttachmentPolicy, targetId: string, operation: AttachmentOperation) { return service.authorizeAttachmentTarget(unit(db), actor, policy, targetId, operation); }
export function assertAttachmentMedia(policy: AttachmentPolicy, mediaType: string, bytes: number) { return service.assertAttachmentMedia(policy, mediaType, bytes); }
