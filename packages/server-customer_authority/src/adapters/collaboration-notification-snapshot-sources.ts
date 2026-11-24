import 'server-only';
/** Closed, named source clauses for the supplied notification statement snapshot.
 * No table, SQL, predicate or alias is accepted from a caller. */
const cols = {
  bindings: '"id", "company_id", "order_id", "customer_id", "email", "expires_at", "status", "reason", "created_by", "approved_by", "reviewed_at", "version", "created_at", "updated_at"',
  reviews: '"id", "company_id", "left_customer_id", "right_customer_id", "status", "canonical_customer_id", "evidence", "created_by", "reviewed_by", "version", "created_at", "updated_at"',
  changes: '"id", "company_id", "customer_id", "expected_customer_version", "previous_json", "proposed_json", "status", "evidence", "created_by", "reviewed_by", "version", "created_at", "updated_at"',
} as const;
/* Single shared projection; the alias is fixed at each closed call site. */
const snapshotFrom = (table: string, columns: string, alias: string) => `FROM (SELECT ${columns} FROM ${table}) ${alias}`;
export function collaborationNotificationSnapshotSources(){return {
  "customer_authority_notifications_customer_access_bindings_b_from": snapshotFrom('customer_access_bindings', cols.bindings, 'b'),
  "customer_authority_notifications_customer_access_bindings_from": snapshotFrom('customer_access_bindings', cols.bindings, 'customer_access_bindings'),
  "customer_authority_notifications_customer_duplicate_reviews_from": snapshotFrom('customer_duplicate_reviews', cols.reviews, 'customer_duplicate_reviews'),
  "customer_authority_notifications_customer_identity_changes_c_from": snapshotFrom('customer_identity_changes', cols.changes, 'c'),
  "customer_authority_notifications_customer_identity_changes_from": snapshotFrom('customer_identity_changes', cols.changes, 'customer_identity_changes'),
  "customer_authority_notifications_customer_duplicate_reviews_r_from": snapshotFrom('customer_duplicate_reviews', cols.reviews, 'r'),
} as const;}
