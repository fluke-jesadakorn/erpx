import "server-only";
/** Closed owner read projections; consumers retain original filters, aliases and binds in their supplied statement/transaction. */
export const lastAuditOwnerProjectionSources = {
  attachments: "SELECT \"id\",\"upload_id\",\"owner_type\",\"owner_id\",\"purpose\",\"object_key\",\"revision\",\"original_name\",\"media_type\",\"bytes\",\"checksum_sha256\",\"created_by\",\"legal_hold\",\"created_at\",\"revoked_at\",\"company_id\",\"detected_media_type\",\"source_state\",\"scanned_at\",\"scanner_definition_at\",\"preview_object_key\",\"preview_media_type\",\"preview_checksum_sha256\",\"preview_bytes\",\"destroyed_at\",\"legacy_sanitized_only\" FROM attachments",
} as const;
