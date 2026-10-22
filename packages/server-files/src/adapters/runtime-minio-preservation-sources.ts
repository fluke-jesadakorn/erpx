import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Fixed Minio preservation facts, evaluated in the caller's active transaction. */
export function runtimeMinioPreservationSources(db:SqlDatabase){return {
ready:()=>db.prepare("SELECT to_regclass('attachment_orphan_preservations') present").first("present"),
pendingOwner:(key:string)=>db.prepare("SELECT 1 FROM attachment_orphan_preservations p WHERE p.object_key=? AND NOT EXISTS(SELECT 1 FROM attachment_orphan_preservation_events e WHERE e.preservation_id=p.id AND e.event_kind='restored')").bind(key).first(),
preservedDerivativeObject:"EXISTS(SELECT 1 FROM attachment_derivatives d WHERE d.attachment_id=p.source_id AND d.object_key=?)"
};}
