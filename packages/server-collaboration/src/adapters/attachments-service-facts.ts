import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { ApiError } from "@erpx/server-runtime/public/http";
import { nativeOwnerWriteSystemUnscoped } from "@erpx/server-collaboration/adapters/native-owner-write";
import { attachmentVersionGuard } from './read-tail-sql';
const allowedTargets = new Set<string>(["messaging_room", "social_draft"]);
const recordVersionTables: Record<string, {
    table: string;
    extra?: string;
}> = { "social_draft": { "table": "social_post_drafts" } };
function ownerVersionGuard(db: SqlDatabase, commandId: string, targetType: string, targetId: string, actor: WorkContext, version: number, index: number) {
    if (!allowedTargets.has(targetType))
        return null;
    const companyId = actor.companyId;
    const aliases: Record<string, {
        table: string;
        extra?: string;
    }> = { "social_draft": { "table": "social_post_drafts" }, "messaging_room": { "table": "messaging_rooms" } };
    const item = aliases[targetType];
    if (!item)
        throw new ApiError(422, "ATTACHMENT_TARGET_UNSUPPORTED", "This file target has no atomic version guard.");
    return assertion(db, commandId, `attachment-owner-${index}`, attachmentVersionGuard(item.table, item.extra ? ` AND ${item.extra}` : ''), [targetId, companyId, version]);
}
function ownerVersionStep(db: SqlDatabase, targetType: string, targetId: string, expectedVersion: number, now: number) {
    if (!allowedTargets.has(targetType))
        return null;
    const descriptor = recordVersionTables[targetType];
    if (!descriptor)
        throw new ApiError(422, "RECORD_ATTACHMENT_BIND_UNSUPPORTED", "This record does not support standalone document binding.");
    return nativeOwnerWriteSystemUnscoped(db, 'attachment_service_version_step', [now, targetId, expectedVersion], `attachment-service-version:${targetId}`, now, { target: descriptor.table === 'social_post_drafts' ? 'social_draft' : undefined });
}
export function attachmentServiceCollaborationFacts(db: SqlDatabase) {
    return { versionGuard: (...args: Parameters<typeof ownerVersionGuard> extends [
            SqlDatabase,
            ...infer Rest
        ] ? Rest : never) => ownerVersionGuard(db, ...args),
        versionStep: (targetType: string, targetId: string, expectedVersion: number, now: number) => ownerVersionStep(db, targetType, targetId, expectedVersion, now) };
}
