import "server-only";
import { ATTACHMENT_LINKS_OWNER_PROJECTION_SQL, ATTACHMENT_SETS_OWNER_PROJECTION_SQL, ATTACHMENTS_OWNER_PROJECTION_SQL } from "./owner-projection-dedupe-sql";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  attachment_derivatives: "SELECT \"id\",\"attachment_id\",\"company_id\",\"page_number\",\"object_key\",\"checksum_sha256\",\"original_checksum\",\"bytes\",\"created_by\",\"created_at\" FROM attachment_derivatives",
  attachment_documents: "SELECT \"attachment_id\",\"company_id\",\"original_checksum\",\"page_count\",\"created_at\" FROM attachment_documents",
  attachment_links: ATTACHMENT_LINKS_OWNER_PROJECTION_SQL,
  attachment_sets: ATTACHMENT_SETS_OWNER_PROJECTION_SQL,
  attachments: ATTACHMENTS_OWNER_PROJECTION_SQL,
} as const;
