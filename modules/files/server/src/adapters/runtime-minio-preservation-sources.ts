import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {ORPHAN_PRESERVATION_PENDING_SQL,ORPHAN_PRESERVATION_PRESENT_SQL} from "./preservation-presence-dedupe-sql";
import {preservedObjectExistsSql} from "./read-tail-r26-source-sql";
/** Fixed Minio preservation facts, evaluated in the caller's active transaction. */
export function runtimeMinioPreservationSources(db:SqlDatabase){return {
ready:()=>db.prepare(ORPHAN_PRESERVATION_PRESENT_SQL).first("present"),
pendingOwner:(key:string)=>db.prepare(ORPHAN_PRESERVATION_PENDING_SQL).bind(key).first(),
preservedDerivativeObject:preservedObjectExistsSql('attachment_derivatives','d','p.source_id')
};}
