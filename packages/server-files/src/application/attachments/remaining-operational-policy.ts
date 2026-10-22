import {remainingOperationalAttachmentPolicies,ownedTargets,moduleScopedTargets} from "@erpx/server-files/domain/attachments/remaining-operational-policy";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { Clock } from "@erpx/server-shared/application/ports";
import type { AttachmentOperation, AttachmentPolicy, AttachmentTargetScope } from "@erpx/server-files/domain/attachments/types";
export type OperationalAttachmentUnit<Effect> = {
    readInventoryTransfers1(facts: {
        targetId: string;
        companyId: string;
    }): Promise<(TargetRow & {
        source_branch_id: string;
        target_branch_id: string;
        status: string;
        created_by: string;
        prepared_by: string;
        delivery_mode: string;
    }) | null>;
    readModuleScoped(facts: {
        targetType: string;
        targetId: string;
        companyId: string;
    }): Promise<TargetRow | null>;
    readCommerceOrders1(facts: {
        targetId: string;
        companyId: string;
    }): Promise<(TargetRow & {
        state: string;
    }) | null>;
    readCommerceOrders2(facts: {
        targetId: string;
        companyId: string;
    }): Promise<TargetRow | null>;
    readProcurementBillMatches1(facts: {
        targetId: string;
        companyId: string;
    }): Promise<TargetRow | null>;
    readInventoryQualitySpecs1(facts: {
        targetId: string;
        companyId: string;
    }): Promise<TargetRow | null>;
    readEcommerceListings1(facts: {
        targetId: string;
        companyId: string;
    }): Promise<TargetRow | null>;
    readMarketingAssets1(facts: {
        targetId: string;
        companyId: string;
    }): Promise<(TargetRow & {
        kind: string;
    }) | null>;
    readManufacturingSpecifications1(facts: {
        targetId: string;
        companyId: string;
    }): Promise<TargetRow | null>;
    versionGuard(commandId: string, targetType: string, targetId: string, companyId: string, version: number, index: number): Effect | null;
    versionStep(targetType: string, targetId: string, expectedVersion: number, now: number): Effect | null;
};
export type TargetRow = {
    company_id: string;
    branch_id: string | null;
    department_id: string | null;
    version: number;
};
export function createOperationalAttachmentPolicy<Effect>(ports: {
    clock: Clock;
    error(status: number, code: string, message: string): Error;
}) {
    function targetScope(row: TargetRow, operation: AttachmentOperation): AttachmentTargetScope {
        return {
            companyId: row.company_id,
            branchId: row.branch_id,
            departmentId: row.department_id,
            version: Number(row.version),
            editable: operation !== "read",
        };
    }
    function requireScope(actor: WorkContext, row: TargetRow) {
        if (actor.branchId && row.branch_id && actor.branchId !== row.branch_id)
            throw ports.error(403, "ATTACHMENT_SCOPE_FORBIDDEN", "The file owner is outside your Branch scope.");
        if (actor.departmentId &&
            row.department_id &&
            actor.departmentId !== row.department_id)
            throw ports.error(403, "ATTACHMENT_SCOPE_FORBIDDEN", "The file owner is outside your Department scope.");
    }
    function requireExactScope(actor: WorkContext, row: TargetRow) {
        if (actor.branchId && actor.branchId !== row.branch_id)
            throw ports.error(403, "ATTACHMENT_SCOPE_FORBIDDEN", "The file owner is outside your Branch scope.");
        if (actor.departmentId && actor.departmentId !== row.department_id)
            throw ports.error(403, "ATTACHMENT_SCOPE_FORBIDDEN", "The file owner is outside your Department scope.");
    }
    async function authorizeRemainingOperationalAttachmentTarget(db: OperationalAttachmentUnit<Effect>, actor: WorkContext, policy: AttachmentPolicy, targetId: string, operation: AttachmentOperation): Promise<AttachmentTargetScope | null> {
        if (policy.targetType === "inventory_transfer" &&
            policy.purposeKey === "direct_receipt_evidence") {
            const row = await db.readInventoryTransfers1({ targetId: targetId, companyId: actor.companyId });
            if (!row)
                throw ports.error(404, "ATTACHMENT_TARGET_NOT_FOUND", "The warehouse transfer was not found in this Company.");
            if (row.delivery_mode !== "direct")
                throw ports.error(422, "ATTACHMENT_PURPOSE_OWNER", "Direct receipt evidence belongs only to a direct warehouse transfer.");
            if (operation === "read") {
                if (!hasPermission(actor, "inventory.read") || actor.departmentId)
                    throw ports.error(403, "ATTACHMENT_FORBIDDEN", "Inventory read authority is required for transfer evidence.");
                if (actor.branchId && ![row.source_branch_id, row.target_branch_id].includes(actor.branchId))
                    throw ports.error(403, "ATTACHMENT_SCOPE_FORBIDDEN", "The transfer is outside your Branch scope.");
            }
            else {
                if (actor.departmentId ||
                    !hasPermission(actor, "inventory.transfer.review") ||
                    row.status !== "submitted" ||
                    samePerson(actor, row.created_by) ||
                    samePerson(actor, row.prepared_by) ||
                    (actor.branchId &&
                        (!hasPermission(actor, "inventory.prepare") ||
                            actor.branchId !== row.target_branch_id)))
                    throw ports.error(403, "ATTACHMENT_FORBIDDEN", "The current independent receiving reviewer is required for direct transfer evidence.");
            }
            return {
                companyId: row.company_id,
                branchId: row.target_branch_id,
                departmentId: null,
                version: Number(row.version),
                editable: operation !== "read",
            };
        }
        const moduleTarget = moduleScopedTargets[policy.targetType as keyof typeof moduleScopedTargets];
        if (moduleTarget) {
            if (!hasPermission(actor, operation === "read" ? "records.read" : "records.write") ||
                !hasModuleAccess(actor.modules, moduleTarget.module))
                throw ports.error(403, "ATTACHMENT_FORBIDDEN", `Your current assignment cannot access ${moduleTarget.module} files.`);
            const row = await db.readModuleScoped({ targetType: policy.targetType, targetId, companyId: actor.companyId });
            if (!row)
                throw ports.error(404, "ATTACHMENT_TARGET_NOT_FOUND", "The file owner was not found in this Company.");
            if (actor.branchId && row.branch_id !== actor.branchId)
                throw ports.error(403, "ATTACHMENT_SCOPE_FORBIDDEN", "The file owner is outside your Branch scope.");
            if (actor.departmentId && row.department_id !== actor.departmentId)
                throw ports.error(403, "ATTACHMENT_SCOPE_FORBIDDEN", "The file owner is outside your Department scope.");
            return targetScope(row, operation);
        }
        if (policy.targetType === "purchase_order" &&
            policy.purposeKey === "supplier_promise") {
            if (!hasPermission(actor, operation === "read" ? "commerce.read" : "commerce.purchase"))
                throw ports.error(403, "ATTACHMENT_FORBIDDEN", "Procurement authority is required for supplier promise evidence.");
            const row = await db.readCommerceOrders1({ targetId: targetId, companyId: actor.companyId });
            if (!row)
                throw ports.error(404, "ATTACHMENT_TARGET_NOT_FOUND", "The purchase order was not found in this Company.");
            requireExactScope(actor, row);
            if (operation !== "read" && !["confirmed", "partially_fulfilled"].includes(row.state))
                throw ports.error(409, "ATTACHMENT_TARGET_STATE", "Supplier promise evidence requires an approved outstanding purchase order.");
            return targetScope(row, operation);
        }
        if (policy.targetType === "commerce_order" &&
            policy.purposeKey === "signed_quote") {
            if (!hasPermission(actor, operation === "read" ? "commerce.read" : "commerce.sales"))
                throw ports.error(403, "ATTACHMENT_FORBIDDEN", "Sales authority is required for signed quotation documents.");
            const row = await db.readCommerceOrders2({ targetId: targetId, companyId: actor.companyId });
            if (!row)
                throw ports.error(404, "ATTACHMENT_TARGET_NOT_FOUND", "The quotation or sales order was not found in this Company.");
            requireExactScope(actor, row);
            return targetScope(row, operation);
        }
        if (!ownedTargets.has(policy.targetType))
            return null;
        let row: TargetRow | null = null;
        if (policy.targetType === "supplier_bill_match") {
            if (!hasPermission(actor, operation === "read" ? "finance.read" : "finance.write"))
                throw ports.error(403, "ATTACHMENT_FORBIDDEN", "Finance authority is required for supplier invoice documents.");
            row = await db.readProcurementBillMatches1({ targetId: targetId, companyId: actor.companyId });
        }
        else if (policy.targetType === "inventory_quality_spec") {
            if (!hasPermission(actor, operation === "read" ? "quality.read" : "quality.release"))
                throw ports.error(403, "ATTACHMENT_FORBIDDEN", "Quality authority is required for controlled specification documents.");
            if (operation !== "read" && (actor.branchId || actor.departmentId))
                throw ports.error(403, "ATTACHMENT_SCOPE_FORBIDDEN", "Use a Company Quality assignment to manage controlled specifications.");
            row = await db.readInventoryQualitySpecs1({ targetId: targetId, companyId: actor.companyId });
        }
        else if (policy.targetType === "ecommerce_listing") {
            if (!hasPermission(actor, operation === "read" ? "commerce.read" : "commerce.sales") || !hasModuleAccess(actor.modules, "ecommerce"))
                throw ports.error(403, "ATTACHMENT_FORBIDDEN", "E-commerce authority is required for listing files.");
            row = await db.readEcommerceListings1({ targetId: targetId, companyId: actor.companyId });
        }
        else if (policy.targetType === "marketing_asset") {
            if (!hasPermission(actor, operation === "read" ? "records.read" : "records.write") || !hasModuleAccess(actor.modules, "marketing"))
                throw ports.error(403, "ATTACHMENT_FORBIDDEN", "Marketing authority is required for asset files.");
            row = await db.readMarketingAssets1({ targetId: targetId, companyId: actor.companyId });
            if (row &&
                ["creative", "consent"].includes(policy.purposeKey) &&
                (row as TargetRow & {
                    kind: string;
                }).kind !== "content")
                throw ports.error(422, "ATTACHMENT_PURPOSE_OWNER", "Creative media and rights evidence belong to a content marketing asset.");
        }
        else if (policy.targetType === "manufacturing_spec") {
            if (!hasPermission(actor, operation === "read" ? "records.read" : "records.write") || !hasModuleAccess(actor.modules, "manufacturing"))
                throw ports.error(403, "ATTACHMENT_FORBIDDEN", "Manufacturing authority is required for controlled specification files.");
            row = await db.readManufacturingSpecifications1({ targetId: targetId, companyId: actor.companyId });
        }
        if (!row)
            throw ports.error(404, "ATTACHMENT_TARGET_NOT_FOUND", "The file owner was not found in this Company.");
        if (policy.targetType === "supplier_bill_match")
            requireExactScope(actor, row);
        else
            requireScope(actor, row);
        return targetScope(row, operation);
    }
    function remainingOperationalAttachmentTargetVersionGuard(db: OperationalAttachmentUnit<Effect>, commandId: string, targetType: string, targetId: string, companyId: string, version: number, index: number): Effect | null { return db.versionGuard(commandId, targetType, targetId, companyId, version, index); }
    function remainingOperationalAttachmentRecordVersionStep(db: OperationalAttachmentUnit<Effect>, targetType: string, targetId: string, expectedVersion: number, now: number): Effect | null { return db.versionStep(targetType, targetId, expectedVersion, now); }
    return { remainingOperationalAttachmentPolicies, authorizeRemainingOperationalAttachmentTarget, remainingOperationalAttachmentTargetVersionGuard, remainingOperationalAttachmentRecordVersionStep };
}
