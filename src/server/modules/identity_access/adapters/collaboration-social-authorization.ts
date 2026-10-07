import { socialDepartmentBranchSql } from '../../organization/public/collaboration-read-projections'; /** Owner-native assignment fence receives only owner-native social source projections. */
export function socialAssignmentAccessSql(alias: string, currentAssignmentFence: string, audience: string, postSources: string) {
    return `EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=? AND a.company_id=${alias}.company_id AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND (
    (${alias}.branch_id IS NULL AND ${alias}.department_id IS NULL)
    OR (${alias}.department_id IS NOT NULL AND a.scope_kind='department' AND a.scope_node_id=${alias}.department_id)
    OR (${alias}.department_id IS NULL AND ${alias}.branch_id IS NOT NULL AND (
      a.scope_kind='holding'
      OR (a.scope_kind='branch' AND a.scope_node_id=${alias}.branch_id)
      OR (a.scope_kind='department' AND ${socialDepartmentBranchSql(alias)})
    ))
  ) AND (
${audience}  ) ${currentAssignmentFence}
  AND ${postSources})`;
}
