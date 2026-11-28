import "server-only";
import {ORG_NODES_SOURCE,ORG_POSITIONS_SOURCE} from './org-sources';
import {ownerProjection} from './organization-column-alias-families-r35';
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const finalRootOwnerProjectionSources = {
  department_heads: ownerProjection('"department_id","company_id","person_id","version","updated_by","updated_at"','department_heads'),
  organization_nodes: ORG_NODES_SOURCE,
  organization_positions: ORG_POSITIONS_SOURCE,
  staff_level_history: ownerProjection('"id","engagement_id","previous_level_id","level_id","actor_id","reason","command_id","created_at","insertion_order"','staff_level_history'),
  staff_levels: ownerProjection('"id","company_id","code","name","rank","active","version","created_at","updated_at","level_number"','staff_levels'),
} as const;
