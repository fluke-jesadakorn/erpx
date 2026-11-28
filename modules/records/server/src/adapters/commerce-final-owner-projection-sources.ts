import "server-only";
import { employeeRequestsProjectionSql } from "./platform-owner-projection-sources";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commerceFinalOwnerProjectionSources = {
  employee_requests: employeeRequestsProjectionSql,
} as const;
