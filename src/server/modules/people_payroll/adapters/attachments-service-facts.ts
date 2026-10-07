import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { WorkContext } from "../../identity_access/public/authorization";
import { assertion } from "../../workflow/public/commands";
import { ApiError } from "../../runtime/public/http";
const allowedTargets = new Set<string>(["expense_claim", "expense_payment", "leave_request"]);
const recordVersionTables: Record<string, {
    table: string;
    extra?: string;
}> = { "expense_claim": { "table": "finance_expenses" }, "expense_payment": { "table": "expense_payment_drafts" } };
function ownerVersionGuard(db: SqlDatabase, commandId: string, targetType: string, targetId: string, actor: WorkContext, version: number, index: number) {
    if (!allowedTargets.has(targetType))
        return null;
    const companyId = actor.companyId;
    const aliases: Record<string, {
        table: string;
        extra?: string;
    }> = { "leave_request": { "table": "leave_requests" }, "expense_claim": { "table": "finance_expenses" }, "expense_payment": { "table": "expense_payment_drafts" } };
    const item = aliases[targetType];
    if (!item)
        throw new ApiError(422, "ATTACHMENT_TARGET_UNSUPPORTED", "This file target has no atomic version guard.");
    return assertion(db, commandId, `attachment-owner-${index}`, `EXISTS(SELECT 1 FROM ${item.table} WHERE id=? AND company_id=? AND version=?${item.extra ? ` AND ${item.extra}` : ""})`, [targetId, companyId, version]);
}
function ownerVersionStep(db: SqlDatabase, targetType: string, targetId: string, expectedVersion: number, now: number) {
    if (!allowedTargets.has(targetType))
        return null;
    if (targetType === "leave_request")
        return db.prepare("UPDATE leave_requests SET version=version+1,evidence_version=evidence_version+1,updated_at=? WHERE id=? AND version=?").bind(now, targetId, expectedVersion);
    const descriptor = recordVersionTables[targetType];
    if (!descriptor)
        throw new ApiError(422, "RECORD_ATTACHMENT_BIND_UNSUPPORTED", "This record does not support standalone document binding.");
    return db.prepare(`UPDATE ${descriptor.table} SET version=version+1${["inventory_lots", "commerce_products", "marketing_campaigns"].includes(descriptor.table) ? "" : ",updated_at=?"} WHERE id=? AND version=?${descriptor.extra ? ` AND ${descriptor.extra}` : ""}`)
        .bind(...(["inventory_lots", "commerce_products", "marketing_campaigns"].includes(descriptor.table) ? [targetId, expectedVersion] : [now, targetId, expectedVersion]));
}
export function attachmentServicePeoplePayrollFacts(db: SqlDatabase) {
    return { versionGuard: (...args: Parameters<typeof ownerVersionGuard> extends [
            SqlDatabase,
            ...infer Rest
        ] ? Rest : never) => ownerVersionGuard(db, ...args),
        versionStep: (targetType: string, targetId: string, expectedVersion: number, now: number) => ownerVersionStep(db, targetType, targetId, expectedVersion, now),
        readLeaveRequests4: (facts: {
            targetId: string;
            companyId: string;
        }) => { return db.prepare("SELECT id FROM leave_requests WHERE id=? AND company_id=? FOR UPDATE").bind(facts.targetId, facts.companyId).first<{
            id: string;
        }>(); },
        readLeaveRequests132: (facts: {
            target_id: string;
            company_id: string;
        }) => { return db.prepare("SELECT id FROM leave_requests WHERE id=? AND company_id=? FOR UPDATE").bind(facts.target_id, facts.company_id).first<{
            id: string;
        }>(); } };
}
