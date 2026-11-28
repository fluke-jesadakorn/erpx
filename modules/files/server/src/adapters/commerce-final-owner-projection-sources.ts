import "server-only";
import { ATTACHMENTS_OWNER_PROJECTION_SQL } from "./owner-projection-dedupe-sql";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commerceFinalOwnerProjectionSources = {
  attachments: ATTACHMENTS_OWNER_PROJECTION_SQL,
} as const;
