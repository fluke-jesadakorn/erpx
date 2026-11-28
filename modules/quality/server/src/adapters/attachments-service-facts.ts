import "server-only";
import type { SqlDatabase, SqlStatement } from "@erpx/platform-runtime-server/public/runtime/types";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import {nativeQualityGuardAssertion,RD_ATTACH_HOLD_CAS} from "./native-quality-read";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
/** The hold attachment CAS guard is planned by the native Quality owner read
 * registry. Neither owner facts module keeps a TypeScript SQL mirror. */
const allowedTargets = new Set<string>(["inventory_quality_hold", "quality_hold"]);
const recordVersionTables: Record<string, {
    table: string;
    extra?: string;
}> = { "quality_hold": { "table": "inventory_quality_holds" }, "inventory_quality_hold": { "table": "inventory_quality_holds" } };
function ownerVersionGuard(db: SqlDatabase, commandId: string, targetType: string, targetId: string, actor: WorkContext, version: number, index: number): SqlStatement | null {
    if (!allowedTargets.has(targetType))
        return null;
    const companyId = actor.companyId;
    if (!recordVersionTables[targetType])
        throw new ApiError(422, "ATTACHMENT_TARGET_UNSUPPORTED", "This file target has no atomic version guard.");
    return nativeQualityGuardAssertion(db, commandId, `attachment-owner-${index}`, RD_ATTACH_HOLD_CAS, [targetId, companyId, version]);
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
export function attachmentServiceQualityFacts(db: SqlDatabase) {
    return { versionGuard: (...args: Parameters<typeof ownerVersionGuard> extends [
            SqlDatabase,
            ...infer Rest
        ] ? Rest : never) => ownerVersionGuard(db, ...args),
        versionStep: (targetType: string, targetId: string, expectedVersion: number, now: number) => ownerVersionStep(db, targetType, targetId, expectedVersion, now) };
}
