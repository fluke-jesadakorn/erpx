import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { WorkContext } from "../../identity_access/public/authorization";
import { assertion } from "../../workflow/public/commands";
import { ApiError } from "../../runtime/public/http";
import { salesRecipientEligibility } from "../../sales/public/sales-recipient";
const allowedTargets = new Set<string>(["crm_account", "crm_opportunity"]);
const recordVersionTables: Record<string, {
    table: string;
    extra?: string;
}> = { "crm_account": { "table": "crm_accounts" }, "crm_opportunity": { "table": "crm_opportunities" } };
function ownerVersionGuard(db: SqlDatabase, commandId: string, targetType: string, targetId: string, actor: WorkContext, version: number, index: number) {
    if (!allowedTargets.has(targetType))
        return null;
    const companyId = actor.companyId;
    if (targetType === "crm_opportunity") {
        const ownerEligible = salesRecipientEligibility("l", "l.owner_id");
        return assertion(db, commandId, `attachment-owner-${index}`, `EXISTS(SELECT 1 FROM crm_opportunities o JOIN crm_leads l ON l.id=o.lead_id
      WHERE o.id=? AND l.company_id=? AND o.version=? AND l.owner_id=?
        AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?)
        AND ${ownerEligible})`, [targetId, companyId, version, actor.userId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId]);
    }
    if (targetType === "crm_account")
        return assertion(db, commandId, `attachment-owner-${index}`, `EXISTS(SELECT 1 FROM crm_accounts account
    WHERE account.id=? AND account.company_id=? AND account.version=? AND (
      ?::text IS NULL OR account.created_by=? OR EXISTS(SELECT 1 FROM crm_leads lead
        WHERE lead.account_id=account.id AND lead.company_id=account.company_id AND lead.branch_id=?
          AND (?::text IS NULL OR lead.department_id=?))))`, [targetId, companyId, version, actor.branchId, actor.userId, actor.branchId, actor.departmentId, actor.departmentId]);
    const aliases: Record<string, {
        table: string;
        extra?: string;
    }> = {};
    const item = aliases[targetType];
    if (!item)
        throw new ApiError(422, "ATTACHMENT_TARGET_UNSUPPORTED", "This file target has no atomic version guard.");
    return assertion(db, commandId, `attachment-owner-${index}`, `EXISTS(SELECT 1 FROM ${item.table} WHERE id=? AND company_id=? AND version=?${item.extra ? ` AND ${item.extra}` : ""})`, [targetId, companyId, version]);
}
function ownerVersionStep(db: SqlDatabase, targetType: string, targetId: string, expectedVersion: number, now: number) {
    if (!allowedTargets.has(targetType))
        return null;
    const descriptor = recordVersionTables[targetType];
    if (!descriptor)
        throw new ApiError(422, "RECORD_ATTACHMENT_BIND_UNSUPPORTED", "This record does not support standalone document binding.");
    return db.prepare(`UPDATE ${descriptor.table} SET version=version+1${["inventory_lots", "commerce_products", "marketing_campaigns"].includes(descriptor.table) ? "" : ",updated_at=?"} WHERE id=? AND version=?${descriptor.extra ? ` AND ${descriptor.extra}` : ""}`)
        .bind(...(["inventory_lots", "commerce_products", "marketing_campaigns"].includes(descriptor.table) ? [targetId, expectedVersion] : [now, targetId, expectedVersion]));
}
export function attachmentServiceCrmFacts(db: SqlDatabase) {
    return { versionGuard: (...args: Parameters<typeof ownerVersionGuard> extends [
            SqlDatabase,
            ...infer Rest
        ] ? Rest : never) => ownerVersionGuard(db, ...args),
        versionStep: (targetType: string, targetId: string, expectedVersion: number, now: number) => ownerVersionStep(db, targetType, targetId, expectedVersion, now) };
}
