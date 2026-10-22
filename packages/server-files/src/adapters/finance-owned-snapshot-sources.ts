import "server-only";
/** Closed native projections with explicit owned columns for named Finance reads.
 * The consumer retains its supplied transaction and one-statement snapshot;
 * this contract accepts no table name, predicate, SQL, or query from a caller. */
export function financeOwnedSnapshotSources(){return {
  "commerce_facts_attachments_proposed_join": "JOIN (SELECT \"id\",\"upload_id\",\"owner_type\",\"owner_id\",\"purpose\",\"object_key\",\"revision\",\"original_name\",\"media_type\",\"bytes\",\"checksum_sha256\",\"created_by\",\"legal_hold\",\"created_at\",\"revoked_at\",\"company_id\",\"detected_media_type\",\"source_state\",\"scanned_at\",\"scanner_definition_at\",\"preview_object_key\",\"preview_media_type\",\"preview_checksum_sha256\",\"preview_bytes\",\"destroyed_at\",\"legacy_sanitized_only\" FROM attachments) proposed",
  "commerce_facts_attachments_used_join": "JOIN (SELECT \"id\",\"upload_id\",\"owner_type\",\"owner_id\",\"purpose\",\"object_key\",\"revision\",\"original_name\",\"media_type\",\"bytes\",\"checksum_sha256\",\"created_by\",\"legal_hold\",\"created_at\",\"revoked_at\",\"company_id\",\"detected_media_type\",\"source_state\",\"scanned_at\",\"scanner_definition_at\",\"preview_object_key\",\"preview_media_type\",\"preview_checksum_sha256\",\"preview_bytes\",\"destroyed_at\",\"legacy_sanitized_only\" FROM attachments) used"
} as const;}
