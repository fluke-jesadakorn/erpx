import "server-only";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  legal_documents: "SELECT \"id\",\"item_id\",\"version\",\"name\",\"media_type\",\"checksum\",\"bytes\",\"body\",\"object_key\",\"locked\",\"created_by\",\"created_at\",\"attachment_link_id\",\"attachment_source_id\" FROM legal_documents",
  legal_file_preservations: "SELECT \"object_key\",\"item_id\",\"source_type\",\"source_id\",\"checksum\",\"created_by\",\"created_at\" FROM legal_file_preservations",
  legal_hold_targets: "SELECT \"hold_id\",\"item_id\" FROM legal_hold_targets",
  legal_holds: "SELECT \"id\",\"item_id\",\"reason\",\"state\",\"created_by\",\"release_requested_by\",\"released_by\",\"created_at\",\"released_at\" FROM legal_holds",
} as const;
