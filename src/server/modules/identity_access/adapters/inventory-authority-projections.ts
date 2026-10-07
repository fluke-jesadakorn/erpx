/** Native authority projections retain the original single-query snapshot and bind order. */
export function inventoryTransitAuthorityColumns(pendingLossEligibility: string) {
  const activeTeam="a.company_id=d.company_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)";
  const sourceTeam=`${activeTeam} AND iam_role_allows(a.role,a.company_id,'inventory.prepare') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=d.source_branch_id))`;
  const receiptTeam=`${activeTeam} AND iam_role_allows(a.role,a.company_id,'inventory.prepare') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=d.target_branch_id)) AND NOT iam_same_person(a.user_id,d.created_by,d.company_id) AND NOT iam_same_person(a.user_id,d.dispatched_by,d.company_id)`;
  const lossTeam=`${activeTeam} AND a.scope_kind='holding' AND iam_role_allows(a.role,a.company_id,'finance.approve') AND NOT iam_same_person(a.user_id,d.dispatched_by,d.company_id) AND ${pendingLossEligibility}`;
  return `(SELECT count(DISTINCT a.user_id) FROM assignments a WHERE ${sourceTeam}) dispatch_candidates,(SELECT count(DISTINCT a.user_id) FROM assignments a WHERE ${receiptTeam}) receipt_candidates,(SELECT count(DISTINCT a.user_id) FROM assignments a WHERE ${lossTeam}) loss_candidates`;
}
export const inventoryNotificationAudience={
  qualityReaders:"FROM assignments a WHERE a.company_id=? AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?)) AND iam_assignment_active(a.id,?) AND iam_role_allows(a.role,a.company_id,'quality.read')",
  landedCostApprovers:"JOIN assignments a ON a.company_id=c.company_id WHERE c.id=? AND a.scope_kind='holding' AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'finance.approve') AND a.user_id<>c.created_by",
  transitReaders:"JOIN assignments a ON a.company_id=d.company_id AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id IN (d.source_branch_id,d.target_branch_id))) AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'inventory.read')",
} as const;
