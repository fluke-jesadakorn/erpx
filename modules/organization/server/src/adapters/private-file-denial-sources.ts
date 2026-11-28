import 'server-only';
import {denialAggregateClause} from './organization-column-alias-families';
/** Closed readonly facts for the selected Company's rejected private-file
 * request. Fixed denial_scope alias; never a permission or owner selection. */
export const privateFileDenialOrganizationWitness=`jsonb_build_object(
 'nodes',${denialAggregateClause('organization_nodes','node')},
 'positions',${denialAggregateClause('organization_positions','position')},
 'seats',(SELECT jsonb_agg(to_jsonb(seat) ORDER BY seat.kind,seat.scope_id) FROM authority_seats seat WHERE seat.company_id=denial_scope.company_id))`;
