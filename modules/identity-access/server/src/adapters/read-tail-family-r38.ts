/** Shared query fragments for the r38 identity_access read-tail dedupes. Each
 * helper owns exactly one statement token and pins the fixed projection/table/
 * predicates; callers inject only the varying alias, whitespace or source
 * fragment so every rendered statement stays byte-identical to its pre-dedupe
 * literal and bind order stays fixed. No lock, write, session authority, native
 * kind or earlier-round helper changes. */

/** Company-authority assignment probe shared by the direct read and its guard.
 * The closed-union shape selects the original gap after the first
 * iam_assignment_active(...) call; the caller owns the trailing LIMIT/`)` so its
 * bind list and wrapper stay exactly where they were. */
export type CompanyAuthorityProbeShape = 'read' | 'guard';
export const companyAuthorityProbeSql = (shape: CompanyAuthorityProbeShape): string => {
  const gap = shape === 'read' ? '\n    ' : ' ';
  return `SELECT 1 FROM assignments WHERE id=? AND company_id=? AND iam_role_allows(role,company_id,?) AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)${gap}AND scope_kind='holding' AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?`;
};

/** Organization node source fragments used by the executive metric labels. */
export type ExecutiveNodeSources = {
  nFrom: string;
  dFrom: string;
  dJoin: string;
  from: string;
};

/** Branch/department CASE label expressions shared by the two executive metric
 * projections. The alias selects x or i; the three statement tokens live here once.
 * Callers keep the status/default labels and bind list inline. */
export const scopeLabel = (alias: string, s: ExecutiveNodeSources) => ({
  branch: `CASE WHEN ${alias}.scope_kind='holding' THEN 'Holding scope' WHEN ${alias}.scope_kind='branch' THEN COALESCE((SELECT n.display_name ${s.nFrom} WHERE n.id=${alias}.scope_node_id AND n.company_id=${alias}.company_id),'Branch') ELSE COALESCE((SELECT b.display_name ${s.dFrom} ${s.dJoin} ON b.id=d.branch_id AND b.company_id=d.company_id WHERE d.id=${alias}.scope_node_id AND d.company_id=${alias}.company_id),'Unassigned') END`,
  department: `CASE WHEN ${alias}.scope_kind='department' THEN COALESCE((SELECT display_name ${s.from} WHERE id=${alias}.scope_node_id AND company_id=${alias}.company_id),'Department') ELSE 'Other scope' END`,
});

/** Branch-scope IN clause shared by the two executive metric projections. The
 * alias selects x or i; the single statement token lives here once. */
export const branchScopeClauseSql = (alias: string, from: string): string =>
  `AND (${alias}.scope_kind='holding' OR (${alias}.scope_kind='branch' AND ${alias}.scope_node_id=?) OR (${alias}.scope_kind='department' AND ${alias}.scope_node_id IN (SELECT id ${from} WHERE company_id=${alias}.company_id AND branch_id=?)))`;

/** Audit event jsonb_agg subquery shared by the two IAM attachment reads. The
 * alias selects review or migration; the single statement token lives here once. */
export const auditAggSql = (alias: string): string =>
  `(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.created_at,event.id) FROM iam_audit event WHERE event.company_id=${alias}.company_id AND event.target_id=${alias}.id)`;
