import 'server-only';
/** Closed explicit-column owner projections for the supplied IAM statement snapshot.
 * No caller-supplied table, SQL, predicate or alias is accepted. */
export function learningTalentSnapshotSources(){return {
  "learning_adapters_executive_metric_projections_organization_nodes_n_from": "FROM (SELECT \"id\", \"kind\", \"slug\", \"display_name\", \"parent_id\", \"company_id\", \"branch_id\", \"status\", \"hierarchy_revision\" FROM organization_nodes) n",
  "talent_adapters_executive_metric_projections_organization_nodes_n_from": "FROM (SELECT \"id\", \"kind\", \"slug\", \"display_name\", \"parent_id\", \"company_id\", \"branch_id\", \"status\", \"hierarchy_revision\" FROM organization_nodes) n"
} as const;}
