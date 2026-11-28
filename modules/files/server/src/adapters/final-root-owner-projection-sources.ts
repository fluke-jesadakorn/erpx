import "server-only";
import { ATTACHMENT_LINKS_OWNER_PROJECTION_SQL, ATTACHMENT_SETS_OWNER_PROJECTION_SQL, ATTACHMENTS_OWNER_PROJECTION_SQL } from "./owner-projection-dedupe-sql";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const finalRootOwnerProjectionSources = {
  attachment_links: ATTACHMENT_LINKS_OWNER_PROJECTION_SQL,
  attachment_sets: ATTACHMENT_SETS_OWNER_PROJECTION_SQL,
  attachments: ATTACHMENTS_OWNER_PROJECTION_SQL,
} as const;
