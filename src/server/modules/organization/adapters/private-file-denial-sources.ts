import 'server-only';
/** Closed readonly facts for the selected Company's rejected private-file
 * request. Fixed denial_scope alias; never a permission or owner selection. */
export const privateFileDenialOrganizationWitness=`jsonb_build_object(
 'nodes',(SELECT jsonb_agg(to_jsonb(node) ORDER BY node.id) FROM organization_nodes node WHERE node.company_id=denial_scope.company_id),
 'positions',(SELECT jsonb_agg(to_jsonb(position) ORDER BY position.id) FROM organization_positions position WHERE position.company_id=denial_scope.company_id),
 'seats',(SELECT jsonb_agg(to_jsonb(seat) ORDER BY seat.kind,seat.scope_id) FROM authority_seats seat WHERE seat.company_id=denial_scope.company_id))`;
