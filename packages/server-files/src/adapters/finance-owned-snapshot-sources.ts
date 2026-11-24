import "server-only";
import {ATTACHMENTS_OWNER_PROJECTION_SQL} from "./owner-projection-dedupe-sql";
/** Closed native projections with explicit owned columns for named Finance reads.
 * The two Finance joins share one byte-identical 26-column projection and differ
 * only by their closed-set alias (proposed/used). Text only: each consumer still
 * renders and prepares its own statement with its own binds; no SQL or relation
 * name is accepted from a caller. */
export function financeOwnedSnapshotSources(){return {
  "commerce_facts_attachments_proposed_join": `JOIN (${ATTACHMENTS_OWNER_PROJECTION_SQL}) proposed`,
  "commerce_facts_attachments_used_join": `JOIN (${ATTACHMENTS_OWNER_PROJECTION_SQL}) used`
} as const;}
