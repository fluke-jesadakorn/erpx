import 'server-only';
/** Closed, named source clauses for the supplied notification statement snapshot.
 * No table, SQL, predicate or alias is accepted from a caller. */
export function collaborationNotificationSnapshotSources(){return {
  "organization_change_notice_organization_node_changes_c_from": "FROM (SELECT \"id\", \"company_id\", \"target_node_id\", \"action\", \"node_kind\", \"slug\", \"display_name\", \"branch_id\", \"expected_node_version\", \"state\", \"version\", \"requested_by\", \"reviewed_by\", \"reason\", \"review_reason\", \"created_at\", \"updated_at\", \"reviewed_at\", \"applied_at\", \"source_branch_id\" FROM organization_node_changes) c",
  "organization_channels_organization_nodes_from": "FROM (SELECT \"id\", \"kind\", \"slug\", \"display_name\", \"parent_id\", \"company_id\", \"branch_id\", \"status\", \"hierarchy_revision\" FROM organization_nodes) organization_nodes",
  "organization_channels_organization_nodes_n_from": "FROM (SELECT \"id\", \"kind\", \"slug\", \"display_name\", \"parent_id\", \"company_id\", \"branch_id\", \"status\", \"hierarchy_revision\" FROM organization_nodes) n",
  "organization_node_channels_organization_nodes_from": "FROM (SELECT \"id\", \"kind\", \"slug\", \"display_name\", \"parent_id\", \"company_id\", \"branch_id\", \"status\", \"hierarchy_revision\" FROM organization_nodes) organization_nodes",
  "people_roster_facts_organization_nodes_from": "FROM (SELECT \"id\", \"kind\", \"slug\", \"display_name\", \"parent_id\", \"company_id\", \"branch_id\", \"status\", \"hierarchy_revision\" FROM organization_nodes) organization_nodes",
  "people_roster_facts_organization_nodes_parent_from": "FROM (SELECT \"id\", \"kind\", \"slug\", \"display_name\", \"parent_id\", \"company_id\", \"branch_id\", \"status\", \"hierarchy_revision\" FROM organization_nodes) parent"
} as const;}
