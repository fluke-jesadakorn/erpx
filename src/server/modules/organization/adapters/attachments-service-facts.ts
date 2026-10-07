import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { WorkContext } from "../../identity_access/public/authorization";
import { assertion } from "../../workflow/public/commands";
import { ApiError } from "../../runtime/public/http";
const allowedTargets = new Set<string>(["branch_brand", "company_storefront_brand", "holding_brand"]);
const recordVersionTables: Record<string, {
    table: string;
    extra?: string;
}> = {};
function ownerVersionGuard(db: SqlDatabase, commandId: string, targetType: string, targetId: string, actor: WorkContext, version: number, index: number) {
    if (!allowedTargets.has(targetType))
        return null;
    const companyId = actor.companyId;
    if (targetType === "holding_brand")
        return assertion(db, commandId, `attachment-owner-${index}`, "?=1 AND EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND kind='company' AND status='active')", [version, companyId]);
    if (targetType === "branch_brand")
        return assertion(db, commandId, `attachment-owner-${index}`, "?=1 AND EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active')", [version, targetId, companyId]);
    if (targetType === "company_storefront_brand")
        return assertion(db, commandId, `attachment-owner-${index}`, "?=1 AND EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND id=? AND kind='company' AND status='active')", [version, targetId, companyId]);
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
export function attachmentServiceOrganizationFacts(db: SqlDatabase) {
    return { versionGuard: (...args: Parameters<typeof ownerVersionGuard> extends [
            SqlDatabase,
            ...infer Rest
        ] ? Rest : never) => ownerVersionGuard(db, ...args),
        versionStep: (targetType: string, targetId: string, expectedVersion: number, now: number) => ownerVersionStep(db, targetType, targetId, expectedVersion, now) };
}
