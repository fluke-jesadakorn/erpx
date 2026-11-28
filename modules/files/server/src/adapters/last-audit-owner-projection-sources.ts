import "server-only";
import { ATTACHMENTS_OWNER_PROJECTION_SQL } from "./owner-projection-dedupe-sql";
/** Closed owner read projections; consumers retain original filters, aliases and binds in their supplied statement/transaction. */
export const lastAuditOwnerProjectionSources = {
  attachments: ATTACHMENTS_OWNER_PROJECTION_SQL,
} as const;
