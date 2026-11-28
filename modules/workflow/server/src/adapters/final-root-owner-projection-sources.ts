import "server-only";
import {approvalInstancesSql,approvalStepsSql} from './approval-projection-sources';
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const finalRootOwnerProjectionSources = {
  approval_instances: approvalInstancesSql,
  approval_steps: approvalStepsSql,
} as const;
