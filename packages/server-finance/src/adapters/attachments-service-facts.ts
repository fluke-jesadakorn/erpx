import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { ApiError } from "@erpx/server-runtime/public/http";
import { nativeFinanceOwnerWriteSystem } from "./native-finance-owner-write";
const allowedTargets = new Set<string>(["invoice_payment", "supplier_bill"]);
function ownerVersionGuard(db: SqlDatabase, commandId: string, targetType: string, targetId: string, actor: WorkContext, version: number, index: number) {
    if (!allowedTargets.has(targetType))
        return null;
    const companyId = actor.companyId;
    const aliases: Record<string, {
        table: string;
        extra?: string;
    }> = { "supplier_bill": { "table": "finance_invoices" }, "invoice_payment": { "table": "finance_invoices" } };
    const item = aliases[targetType];
    if (!item)
        throw new ApiError(422, "ATTACHMENT_TARGET_UNSUPPORTED", "This file target has no atomic version guard.");
    return assertion(db, commandId, `attachment-owner-${index}`, `EXISTS(SELECT 1 FROM ${item.table} WHERE id=? AND company_id=? AND version=?${item.extra ? ` AND ${item.extra}` : ""})`, [targetId, companyId, version]);
}
function ownerVersionStep(db: SqlDatabase, targetType: string, targetId: string, expectedVersion: number, now: number) {
    if (!allowedTargets.has(targetType))
        return null;
    // Native Finance owner-write executes this version step in Rust with the
    // guard, receipt/audit/outbox in the caller unit. No write SQL remains in
    // TypeScript; the reply is an inert sequencing placeholder.
    return nativeFinanceOwnerWriteSystem(
        db,
        "system",
        `finance_attachment_version_step_${targetType}`,
        [now, targetId, expectedVersion],
        `avs-${targetId}-${expectedVersion}`,
        now,
    );
}
export function attachmentServiceFinanceFacts(db: SqlDatabase) {
    return { versionGuard: (...args: Parameters<typeof ownerVersionGuard> extends [
            SqlDatabase,
            ...infer Rest
        ] ? Rest : never) => ownerVersionGuard(db, ...args),
        versionStep: (targetType: string, targetId: string, expectedVersion: number, now: number) => ownerVersionStep(db, targetType, targetId, expectedVersion, now) };
}
