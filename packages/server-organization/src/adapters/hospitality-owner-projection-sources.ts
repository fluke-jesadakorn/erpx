import "server-only";
import {ORG_NODES_SOURCE,ORG_POSITIONS_SOURCE} from './org-sources';
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  organization_nodes: ORG_NODES_SOURCE,
  organization_positions: ORG_POSITIONS_SOURCE,
} as const;
