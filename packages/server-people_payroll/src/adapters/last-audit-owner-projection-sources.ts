import { PP_RD22_LEAVE_REQUESTS_PROJECTION_SQL } from './pp-read-tail-r22-sources';
import "server-only";
/** Closed owner read projections; consumers retain original filters, aliases and binds in their supplied statement/transaction. */
export const lastAuditOwnerProjectionSources = {
  leave_requests: PP_RD22_LEAVE_REQUESTS_PROJECTION_SQL,
} as const;
