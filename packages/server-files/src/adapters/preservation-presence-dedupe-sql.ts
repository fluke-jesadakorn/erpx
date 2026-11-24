/** Byte-identical orphan-preservation SQL shared by the read-tail call sites.
 * Text only: every call site still prepares and executes its own statement in
 * its supplied database/transaction and no result is cached here. */
export const ORPHAN_PRESERVATION_PRESENT_SQL = "SELECT to_regclass('attachment_orphan_preservations') present";
export const ORPHAN_PRESERVATION_PENDING_SQL = "SELECT 1 FROM attachment_orphan_preservations p WHERE p.object_key=? AND NOT EXISTS(SELECT 1 FROM attachment_orphan_preservation_events e WHERE e.preservation_id=p.id AND e.event_kind='restored')";
