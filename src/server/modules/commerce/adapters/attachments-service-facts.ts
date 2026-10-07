import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { WorkContext } from "../../identity_access/public/authorization";
import { assertion } from "../../workflow/public/commands";
import { ApiError } from "../../runtime/public/http";
const allowedTargets = new Set<string>(["commerce_fulfillment","commerce_order", "commerce_order_delivery", "commerce_return", "product", "purchase_order", "purchase_receipt"]);
const recordVersionTables: Record<string, {
    table: string;
    extra?: string;
}> = { "purchase_order": { "table": "commerce_orders", "extra": "kind='purchase'" }, "product": { "table": "commerce_products" }, "commerce_order": { "table": "commerce_orders" }, "commerce_order_delivery": { "table": "commerce_orders" }, "commerce_return": { "table": "commerce_returns" } };
function ownerVersionGuard(db: SqlDatabase, commandId: string, targetType: string, targetId: string, actor: WorkContext, version: number, index: number) {
    if (!allowedTargets.has(targetType))
        return null;
    const companyId = actor.companyId;
  if(targetType==="commerce_fulfillment")return assertion(db,commandId,`attachment-owner-${index}`,"EXISTS(SELECT 1 FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE f.id=? AND o.company_id=? AND f.version=?)",[targetId,companyId,version]);
    const aliases: Record<string, {
        table: string;
        extra?: string;
    }> = { "purchase_receipt": { "table": "commerce_orders", "extra": "kind='purchase'" }, "purchase_order": { "table": "commerce_orders", "extra": "kind='purchase'" }, "product": { "table": "commerce_products" }, "commerce_order": { "table": "commerce_orders" }, "commerce_order_delivery": { "table": "commerce_orders" }, "commerce_return": { "table": "commerce_returns" } };
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
export function attachmentServiceCommerceFacts(db: SqlDatabase) {
    return { versionGuard: (...args: Parameters<typeof ownerVersionGuard> extends [
            SqlDatabase,
            ...infer Rest
        ] ? Rest : never) => ownerVersionGuard(db, ...args),
        versionStep: (targetType: string, targetId: string, expectedVersion: number, now: number) => ownerVersionStep(db, targetType, targetId, expectedVersion, now) };
}
