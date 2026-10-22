import "server-only";
/** Closed native projections for commercial owner joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commercialOwnerProjectionSources = {
  attachment_publications: "SELECT \"id\",\"company_id\",\"source_link_id\",\"target_type\",\"target_id\",\"derivative_object_key\",\"derivative_media_type\",\"derivative_checksum_sha256\",\"derivative_bytes\",\"alt_text\",\"ordinal\",\"revision\",\"state\",\"created_by\",\"created_at\",\"approved_by\",\"approved_at\",\"revoked_by\",\"revoked_at\",\"revocation_reason\" FROM attachment_publications",
} as const;
