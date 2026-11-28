import "server-only";
import {ORG_NODES_SOURCE} from './org-sources';
/** Closed native projections for commercial owner joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commercialOwnerProjectionSources = {
  organization_nodes: ORG_NODES_SOURCE,
} as const;
