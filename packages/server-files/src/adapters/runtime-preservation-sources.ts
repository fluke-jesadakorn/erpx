import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Read-only source predicates composed by Legal into its single, transaction-scoped hold check. */
export function runtimePreservationFileSources(db:SqlDatabase){return {
 async pendingOwner(key:string){if(!await db.prepare("SELECT to_regclass('attachment_orphan_preservations') present").first("present"))return false;return !!await db.prepare("SELECT 1 FROM attachment_orphan_preservations p WHERE p.object_key=? AND NOT EXISTS(SELECT 1 FROM attachment_orphan_preservation_events e WHERE e.preservation_id=p.id AND e.event_kind='restored')").bind(key).first();},
 preservedAttachmentObject:"EXISTS(SELECT 1 FROM attachment_storage_ledger storage WHERE storage.attachment_id=p.source_id AND storage.object_key=?)",
 boundLegalItemIds:`SELECT owner.target_id FROM attachments source JOIN attachment_links link ON link.attachment_id=source.id JOIN attachment_sets owner ON owner.id=link.set_id WHERE owner.target_type='legal_item' AND (source.object_key=? OR source.preview_object_key=? OR EXISTS(SELECT 1 FROM attachment_storage_ledger storage WHERE storage.attachment_id=source.id AND storage.object_key=?))`
};}
