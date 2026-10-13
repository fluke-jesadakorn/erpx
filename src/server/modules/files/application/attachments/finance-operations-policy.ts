import { financeOperationsAttachmentPolicies, targetTypes } from "../../domain/attachments/finance-operations-policy";
import { hasPermission } from "@/lib/identity_access/permissions";
import type { WorkContext } from "../../../identity_access/public/authorization";
import type { Clock } from "../../../shared/application/ports";
import type { AttachmentOperation, AttachmentPolicy, AttachmentTargetScope } from "../../domain/attachments/types";
export type FinanceAttachmentUnit<Effect> = {
    readOwner1(facts: {
        companyId: string;
        targetId: string;
        targetType: string;
        purposeKey: string;
    }): Promise<Row | null>;
    readOwner2(facts: {
        companyId: string;
        targetId: string;
        targetType: string;
        purposeKey: string;
    }): Promise<Row | null>;
    readOwner3(facts: {
        companyId: string;
        targetId: string;
        targetType: string;
        purposeKey: string;
    }): Promise<Row | null>;
    readOwner4(facts: {
        companyId: string;
        targetId: string;
        targetType: string;
        purposeKey: string;
    }): Promise<Row | null>;
    readOwner5(facts: {
        companyId: string;
        targetId: string;
        targetType: string;
        purposeKey: string;
    }): Promise<Row | null>;
    readOwner6(facts: {
        companyId: string;
        targetId: string;
        targetType: string;
        purposeKey: string;
    }): Promise<Row | null>;
    readOwner7(facts: {
        companyId: string;
        targetId: string;
        targetType: string;
        purposeKey: string;
    }): Promise<Row | null>;
    readOwner8(facts: {
        companyId: string;
        targetId: string;
        targetType: string;
        purposeKey: string;
    }): Promise<Row | null>;
    readOwner9(facts: {
        companyId: string;
        targetId: string;
        targetType: string;
        purposeKey: string;
    }): Promise<Row | null>;
    readOwner10(facts: {
        companyId: string;
        targetId: string;
        targetType: string;
        purposeKey: string;
    }): Promise<Row | null>;
    readOwner11(facts: {
        companyId: string;
        targetId: string;
        targetType: string;
        purposeKey: string;
    }): Promise<Row | null>;
    readOwner12(facts: {
        companyId: string;
        targetId: string;
        targetType: string;
        purposeKey: string;
    }): Promise<Row | null>;
    overtime(actor: WorkContext, id: string): Promise<{
        created_by: string;
        approval: {
            canDecide: boolean;
            canOverride: boolean;
        };
        company_id: string;
        branch_id: string | null;
        department_id: string | null;
        version: number;
    }>;
    versionGuard(commandId: string, targetType: string, targetId: string, companyId: string, version: number, index: number): Effect | null;
    versionStep(targetType: string, targetId: string, expectedVersion: number, now: number): Effect | null;
};
export type Row = {
    company_id: string;
    branch_id: string | null;
    department_id: string | null;
    version: number;
    kind?: string;
    created_by?: string;
    requested_by?: string;
    submitted_by?: string;
    state?: string;
    status?: string;
    party_id?: string | null;
};
export function createFinanceAttachmentPolicy<Effect>(ports: {
    clock: Clock;
    error(status: number, code: string, message: string): Error;
}) {
    function forbidden(message = "Your current assignment cannot access files for this record."): never {
        throw ports.error(403, "ATTACHMENT_FORBIDDEN", message);
    }
    function assertWorkspaceScope(actor: WorkContext, row: Row) {
        if (actor.branchId && row.branch_id && actor.branchId !== row.branch_id)
            throw ports.error(403, "ATTACHMENT_SCOPE_FORBIDDEN", "The file owner is outside your Branch scope.");
        if (actor.departmentId && row.department_id && actor.departmentId !== row.department_id)
            throw ports.error(403, "ATTACHMENT_SCOPE_FORBIDDEN", "The file owner is outside your Department scope.");
    }
    function result(row: Row, operation: AttachmentOperation): AttachmentTargetScope {
        return {
            companyId: row.company_id,
            branchId: row.branch_id,
            departmentId: row.department_id,
            version: Number(row.version),
            editable: operation !== "read",
        };
    }
    async function ownerRow(actor: WorkContext, row: Row | null): Promise<Row> {
        if (!row)
            throw ports.error(404, "ATTACHMENT_TARGET_NOT_FOUND", "The file owner was not found in this Company.");
        assertWorkspaceScope(actor, row);
        return row;
    }
    const financeRecords: Record<string, {
        write: string;
    }> = { "finance_billing_correction": { "write": "finance.write" }, "finance_business_bill": { "write": "finance.write" }, "finance_plan_edition": { "write": "finance.write" }, "finance_book_rule": { "write": "finance.write" }, "finance_book_adjustment": { "write": "finance.write" }, "finance_bank_match_group": { "write": "finance.write" }, "finance_collection_case": { "write": "finance.write" }, "finance_business_contract": { "write": "finance.write" }, "finance_recognition_run": { "write": "finance.write" }, "finance_instrument": { "write": "finance.write" }, "finance_instrument_run": { "write": "finance.write" }, "finance_asset_policy": { "write": "finance.write" }, "finance_asset_policy_run": { "write": "finance.write" }, "finance_group_report": { "write": "finance.write" }, "finance_ownership_interest": { "write": "finance.write" }, "finance_acquisition": { "write": "finance.write" }, "finance_intercompany_pair": { "write": "finance.write" }, "finance_journal": { "write": "finance.write" }, "finance_bank_statement": { "write": "finance.write" }, "finance_bank_transaction": { "write": "finance.write" }, "finance_period": { "write": "finance.write" }, "finance_accrual_schedule": { "write": "finance.write" }, "finance_asset": { "write": "finance.write" }, "finance_tax_identity": { "write": "finance.write" }, "finance_lease": { "write": "finance.write" }, "finance_loan": { "write": "finance.write" }, "finance_revenue_contract": { "write": "finance.write" }, "finance_payment": { "write": "finance.pay" }, "finance_refund_obligation": { "write": "finance.pay" }, "finance_supplier_batch": { "write": "finance.pay" } };
    const pmsKinds: Record<string, string> = { "pms_room_type": "room_type", "pms_guest": "guest", "pms_reservation": "reservation", "pms_stay": "stay", "pms_task": "task", "pms_lost_property": "lost" };
    function pmsWritePermission(policy: AttachmentPolicy) {
        if (policy.purposeKey === "booking_media")
            return "pms.configure";
        if (["pms_guest", "pms_reservation"].includes(policy.targetType))
            return "pms.reservations";
        if (policy.targetType === "pms_stay")
            return "pms.frontdesk";
        if (policy.targetType === "pms_task")
            return "pms.housekeeping";
        if (policy.targetType === "pms_lost_property")
            return "pms.housekeeping";
        if (policy.targetType === "pms_property" && policy.purposeKey === "ota_reconciliation")
            return "pms.cashier";
        return "pms.audit";
    }
    async function authorizeFinanceOperationsAttachmentTarget(db: FinanceAttachmentUnit<Effect>, actor: WorkContext, policy: AttachmentPolicy, targetId: string, operation: AttachmentOperation): Promise<AttachmentTargetScope | null> {
        if (!targetTypes.has(policy.targetType))
            return null;
        const finance = financeRecords[policy.targetType];
        if (finance) {
            const permission = operation === "read"
                ? "finance.read"
                : policy.targetType === "finance_supplier_batch" && policy.purposeKey === "payment_batch_support"
                    ? "finance.write"
                    : finance.write;
            if (!hasPermission(actor, permission))
                forbidden();
            if (operation !== "read" && (["finance_period", "finance_payment", "finance_supplier_batch"].includes(policy.targetType) || (policy.targetType === "finance_refund_obligation" && policy.purposeKey === "refund_proof")) && (actor.branchId || actor.departmentId))
                forbidden("Use a Company Finance assignment to manage this evidence.");
            const row = await ownerRow(actor, await db.readOwner1({ companyId: actor.companyId, targetId, targetType: policy.targetType, purposeKey: policy.purposeKey }));
            if (policy.purposeKey === "finance_support") {
                if (["finance_group_report", "finance_ownership_interest", "finance_acquisition", "finance_intercompany_pair"].includes(policy.targetType) && actor.scope.kind !== "holding")
                    forbidden("Use Holding Finance authority to manage group evidence.");
                if (operation !== "read" && !["draft", ...(policy.targetType === "finance_bank_match_group" ? ["proposed"] : [])].includes(String(row.status)))
                    throw ports.error(409, "ATTACHMENT_TARGET_LOCKED", "Retained Finance evidence is locked after review. Prepare a corrected owner edition before changing its documents.");
            }
            if (policy.targetType === "finance_tax_identity") {
                if (actor.branchId || actor.departmentId || row.party_id !== null)
                    forbidden("Scanned signer evidence belongs to the Company payer's tax identity.");
                if (operation !== "read" && row.status !== "draft")
                    throw ports.error(409, "ATTACHMENT_TARGET_LOCKED", "Prepare a new tax identity revision to change an approved signer source.");
            }
            if (policy.targetType === "finance_period" && Number(row.version) < 1)
                throw ports.error(409, "ATTACHMENT_TARGET_NOT_READY", "Begin the period close before attaching close workpapers.");
            return result(row, operation);
        }
        if (["pos_sale", "pos_refund", "pos_shift"].includes(policy.targetType)) {
            const writePermission = policy.targetType === "pos_sale" ? "pos.sell" : "pos.approve";
            if (!hasPermission(actor, operation === "read" ? "pos.read" : writePermission))
                forbidden();
            const row = policy.targetType === "pos_refund"
                ? await ownerRow(actor, await db.readOwner2({ companyId: actor.companyId, targetId, targetType: policy.targetType, purposeKey: policy.purposeKey }))
                : await ownerRow(actor, await db.readOwner2({ companyId: actor.companyId, targetId, targetType: policy.targetType, purposeKey: policy.purposeKey }));
            return result(row, operation);
        }
        if (policy.targetType in pmsKinds) {
            if (!hasPermission(actor, operation === "read" ? "pms.read" : pmsWritePermission(policy)))
                forbidden();
            const row = await ownerRow(actor, await db.readOwner4({ companyId: actor.companyId, targetId, targetType: policy.targetType, purposeKey: policy.purposeKey }));
            if (row.kind !== pmsKinds[policy.targetType])
                throw ports.error(404, "ATTACHMENT_TARGET_NOT_FOUND", "The selected hotel record does not match this file owner type.");
            return result(row, operation);
        }
        if (policy.targetType === "pms_property") {
            if (!hasPermission(actor, operation === "read" ? "pms.read" : pmsWritePermission(policy)))
                forbidden();
            return result(await ownerRow(actor, await db.readOwner5({ companyId: actor.companyId, targetId, targetType: policy.targetType, purposeKey: policy.purposeKey })), operation);
        }
        if (policy.targetType === "overtime_request") {
            const detail = await db.overtime(actor, targetId);
            const own = detail.created_by === actor.userId;
            if (operation !== "read" && !own && !detail.approval.canDecide && !detail.approval.canOverride)
                forbidden("Only the requester or current eligible reviewer can manage overtime evidence.");
            return result({ company_id: detail.company_id, branch_id: detail.branch_id, department_id: detail.department_id, version: detail.version }, operation);
        }
        if (policy.targetType === "time_entry") {
            const row = await ownerRow(actor, await db.readOwner6({ companyId: actor.companyId, targetId, targetType: policy.targetType, purposeKey: policy.purposeKey }));
            const owner = row.submitted_by === actor.userId;
            if (operation === "read") {
                if (!owner && !hasPermission(actor, "people.time.manage"))
                    forbidden();
            }
            else if (!owner || !["draft", "submitted", "rejected"].includes(String(row.status))) {
                forbidden("Only the employee who submitted an editable time entry can attach correction proof.");
            }
            return result(row, operation);
        }
        if (policy.targetType === "organization_node") {
            if (!hasPermission(actor, "organization.manage"))
                forbidden();
            if (operation !== "read" && actor.assignmentScope !== "holding")
                forbidden("Use Company Organization authority to manage official organization documents.");
            const row = await ownerRow(actor, await db.readOwner7({ companyId: actor.companyId, targetId, targetType: policy.targetType, purposeKey: policy.purposeKey }));
            if (policy.purposeKey === "branch_permit" && !["branch", "headquarters"].includes(String(row.kind)))
                throw ports.error(422, "ATTACHMENT_PURPOSE_OWNER", "A branch permit may be attached only to a Branch or Headquarters record.");
            return result(row, operation);
        }
        if (policy.targetType === "organization_change") {
            if (!hasPermission(actor, "organization.manage") || actor.assignmentScope !== "holding")
                forbidden("Use Company Organization authority to manage organization-change evidence.");
            return result(await ownerRow(actor, await db.readOwner8({ companyId: actor.companyId, targetId, targetType: policy.targetType, purposeKey: policy.purposeKey })), operation);
        }
        if (["iam_review", "iam_migration"].includes(policy.targetType)) {
            if (!hasPermission(actor, "iam.manage") || actor.assignmentScope !== "holding")
                forbidden("Company IAM authority is required for sanitized access evidence.");
            return result(await ownerRow(actor, await db.readOwner9({ companyId: actor.companyId, targetId, targetType: policy.targetType, purposeKey: policy.purposeKey })), operation);
        }
        if (policy.targetType === "company_release_acceptance") {
            if (!hasPermission(actor, operation === "read" ? "records.read" : "organization.manage") || actor.assignmentScope !== "holding")
                forbidden("Company release evidence requires Company authority.");
            return result(await ownerRow(actor, await db.readOwner10({ companyId: actor.companyId, targetId, targetType: policy.targetType, purposeKey: policy.purposeKey })), operation);
        }
        if (policy.targetType === "people_provision_operation") {
            if (!hasPermission(actor, operation === "read" ? "iam.read" : "iam.directory.apply") || actor.assignmentScope !== "holding")
                forbidden("Company IT provisioning authority is required.");
            return result(await ownerRow(actor, await db.readOwner11({ companyId: actor.companyId, targetId, targetType: policy.targetType, purposeKey: policy.purposeKey })), operation);
        }
        if (policy.targetType === "company_provisioning") {
            if (!hasPermission(actor, "organization.manage") || actor.assignmentScope !== "holding")
                forbidden("Company provisioning evidence requires Company authority.");
            return result(await ownerRow(actor, await db.readOwner12({ companyId: actor.companyId, targetId, targetType: policy.targetType, purposeKey: policy.purposeKey })), operation);
        }
        return null;
    }
    const forbiddenAttachmentPurposeKeys = Object.freeze([
        "password",
        "private_key",
        "secret",
        "config_dump",
        "database_dump",
    ]);
    const sanitizedEvidenceTargets = new Set([
        "iam_review",
        "iam_migration",
        "company_release_acceptance",
        "people_provision_operation",
        "company_provisioning",
    ]);
    function requiresSanitizedEvidenceAttestation(targetType: string) {
        return sanitizedEvidenceTargets.has(targetType);
    }
    /** Filename/caption defense is intentionally stricter than MIME validation. */
    function assertSanitizedEvidenceMetadata(policy: AttachmentPolicy, filename: string, caption: string | null) {
        if (!requiresSanitizedEvidenceAttestation(policy.targetType))
            return;
        const label = `${filename}\n${caption ?? ""}`.normalize("NFKC").toLocaleLowerCase();
        const unsafe = [
            /(^|[^a-z])(password|passwd|secret|credential)s?([^a-z]|$)/,
            /private[\s_.-]*key/,
            /(^|[/\\._-])id_(rsa|dsa|ecdsa|ed25519)([^a-z]|$)/,
            /(^|[/\\._-])?\.env(?:[^a-z0-9]|$)/,
            /(config|configuration)[\s_.-]*(dump|export|backup)/,
            /(database|db)[\s_.-]*(dump|export|backup)/,
            /\.(pem|key|p12|pfx|sql|dump|bak|sqlite|sqlite3|db)([^a-z0-9]|$)/,
        ].some((pattern) => pattern.test(label));
        if (unsafe)
            throw ports.error(422, "ATTACHMENT_SENSITIVE_SYSTEM_MATERIAL", "Passwords, private keys, secrets, configuration exports and database dumps must never be uploaded. Attach only a sanitized review artifact.");
    }
    function financeOperationsAttachmentTargetVersionGuard(db: FinanceAttachmentUnit<Effect>, commandId: string, targetType: string, targetId: string, companyId: string, version: number, index: number): Effect | null { return db.versionGuard(commandId, targetType, targetId, companyId, version, index); }
    function financeOperationsAttachmentRecordVersionStep(db: FinanceAttachmentUnit<Effect>, targetType: string, targetId: string, expectedVersion: number, now: number): Effect | null { return db.versionStep(targetType, targetId, expectedVersion, now); }
    return { financeOperationsAttachmentPolicies, forbiddenAttachmentPurposeKeys, authorizeFinanceOperationsAttachmentTarget, financeOperationsAttachmentTargetVersionGuard, financeOperationsAttachmentRecordVersionStep, requiresSanitizedEvidenceAttestation, assertSanitizedEvidenceMetadata };
}
