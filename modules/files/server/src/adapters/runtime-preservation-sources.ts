import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import { ORPHAN_PRESERVATION_PENDING_SQL, ORPHAN_PRESERVATION_PRESENT_SQL } from "./preservation-presence-dedupe-sql";
import { preservedObjectExistsSql } from "./read-tail-r26-source-sql";
/** Read-only source predicates composed by Legal into its single, transaction-scoped hold check. */
export function runtimePreservationFileSources(db:SqlDatabase){return {
 async pendingOwner(key:string){if(!await db.prepare(ORPHAN_PRESERVATION_PRESENT_SQL).first("present"))return false;return !!await db.prepare(ORPHAN_PRESERVATION_PENDING_SQL).bind(key).first();},
 preservedAttachmentObject:preservedObjectExistsSql('attachment_storage_ledger','storage','p.source_id'),
 boundLegalItemIds:`SELECT owner.target_id FROM attachments source JOIN attachment_links link ON link.attachment_id=source.id JOIN attachment_sets owner ON owner.id=link.set_id WHERE owner.target_type='legal_item' AND (source.object_key=? OR source.preview_object_key=? OR ${preservedObjectExistsSql('attachment_storage_ledger','storage','source.id')})`
};}
