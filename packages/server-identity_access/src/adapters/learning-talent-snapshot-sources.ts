import 'server-only';
/** Closed explicit-column owner projections for the supplied IAM statement snapshot.
 * No caller-supplied table, SQL, predicate or alias is accepted. */
export function learningTalentSnapshotSources(){return {
  "learning_adapters_learning_assignments_x_from": "FROM (SELECT \"id\", \"user_id\", \"role\", \"modules_json\", \"company_id\", \"branch_id\", \"valid_from\", \"valid_until\", \"revoked_at\", \"version\", \"scope_kind\", \"scope_node_id\", \"parent_id\", \"source\", \"source_id\" FROM assignments) x",
  "talent_adapters_talent_assignments_x_from": "FROM (SELECT \"id\", \"user_id\", \"role\", \"modules_json\", \"company_id\", \"branch_id\", \"valid_from\", \"valid_until\", \"revoked_at\", \"version\", \"scope_kind\", \"scope_node_id\", \"parent_id\", \"source\", \"source_id\" FROM assignments) x",
  "talent_adapters_talent_assignments_from": "FROM (SELECT \"id\", \"user_id\", \"role\", \"modules_json\", \"company_id\", \"branch_id\", \"valid_from\", \"valid_until\", \"revoked_at\", \"version\", \"scope_kind\", \"scope_node_id\", \"parent_id\", \"source\", \"source_id\" FROM assignments) assignments"
} as const;}
