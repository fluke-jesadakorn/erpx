import { finalRootOwnerProjectionSources as filesProjectionSources } from "@erpx/server-files/public/final-root-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { ApiError } from "@erpx/server-runtime/public/http";
const allowedTargets = new Set<string>(["legal_item"]);
const recordVersionTables: Record<string, {
    table: string;
    extra?: string;
}> = {};
function ownerVersionGuard(db: SqlDatabase, commandId: string, targetType: string, targetId: string, actor: WorkContext, version: number, index: number) {
    if (!allowedTargets.has(targetType))
        return null;
    const companyId = actor.companyId;
    const aliases: Record<string, {
        table: string;
        extra?: string;
    }> = { "legal_item": { "table": "legal_items" } };
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
export function attachmentServiceLegalFacts(db: SqlDatabase) {
    return { versionGuard: (...args: Parameters<typeof ownerVersionGuard> extends [
            SqlDatabase,
            ...infer Rest
        ] ? Rest : never) => ownerVersionGuard(db, ...args),
        versionStep: (targetType: string, targetId: string, expectedVersion: number, now: number) => ownerVersionStep(db, targetType, targetId, expectedVersion, now),
        readLegalFilePreservations125: (facts: {
            attachmentId: string;
        }) => {
            return db.prepare(`SELECT 1 FROM (
    SELECT p.item_id FROM legal_file_preservations p
    JOIN legal_hold_targets target ON target.item_id=p.item_id
    JOIN legal_holds hold ON hold.id=target.hold_id
    WHERE p.source_type='attachment' AND p.source_id=? AND hold.state<>'released'
    UNION ALL
    SELECT set_row.target_id FROM (${filesProjectionSources.attachment_links}) link
    JOIN (${filesProjectionSources.attachment_sets}) set_row ON set_row.id=link.set_id
    JOIN legal_hold_targets target ON target.item_id=set_row.target_id
    JOIN legal_holds hold ON hold.id=target.hold_id
    WHERE link.attachment_id=? AND set_row.target_type='legal_item' AND hold.state<>'released'
  ) active LIMIT 1`).bind(facts.attachmentId, facts.attachmentId).first();
        } };
}
