import "server-only";
import { assignmentsProjectionCompact } from './assignment-projection-source';
/** Closed native projections for commercial owner joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commercialOwnerProjectionSources = {
  assignments: assignmentsProjectionCompact,
} as const;
