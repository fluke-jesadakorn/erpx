

import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { handoverSourcesReadableSql } from '@erpx/server-operations/public/handover-policy';
import { pmsPostSourcesReadableSql } from '@erpx/server-pms/public/pms/source-policy';

import { socialScopeKind } from '@erpx/server-collaboration/domain/social-source';

import { socialAssignmentAccessSql } from '@erpx/server-identity_access/public/collaboration-social-authorization';
import { socialProjectAudienceSql } from '@erpx/server-projects/public/project-read-projections';

export function sourceAccessSql(alias = 'p', currentAssignment?: string,viewer?:{userId:string;now:string}) {
    const audience = `    ${alias}.audience_kind IS NULL OR ${alias}.audience_kind IN ('company','branch','department')
    OR (${alias}.audience_kind='participants' AND EXISTS(
      SELECT 1 FROM social_post_participants sp WHERE sp.post_id=${alias}.id AND sp.user_id=a.user_id))
    OR (${alias}.audience_kind='team' AND EXISTS(
      SELECT 1 FROM messaging_teams mt JOIN messaging_team_members mm ON mm.team_id=mt.id
      WHERE mt.id=${alias}.audience_id AND mt.company_id=${alias}.company_id
      AND NOT mt.organization_managed AND mm.user_id=a.user_id))
    OR (${alias}.audience_kind='project' AND EXISTS(
      ${socialProjectAudienceSql(alias)}))
`;
    return socialAssignmentAccessSql(alias, currentAssignment ? `AND (NOT EXISTS(SELECT 1 FROM social_post_sources ps WHERE ps.post_id=${alias}.id AND ps.source_kind='pms_record') OR a.id=${currentAssignment})` : '', audience, `${handoverSourcesReadableSql(alias, 'a.user_id')} AND ${pmsPostSourcesReadableSql(alias, 'a.user_id', 'a.id')}`,viewer);
}
export function socialPostScope(actor: Pick<WorkContext, "companyId" | "userId" | "branchId" | "departmentId"> & Partial<Pick<WorkContext, "assignmentId">>, alias = "p") {
    const now = Date.now(), base = `${alias}.company_id=? AND ${alias}.state='published' AND ${sourceAccessSql(alias, actor.assignmentId ? "?::text" : undefined)}`;
    const values = [actor.companyId, actor.userId, now, now, ...(actor.assignmentId ? [actor.assignmentId] : [])];
    return socialScopeKind(actor) === 'department'
        ? { sql: `${base} AND (${alias}.department_id=? OR (${alias}.department_id IS NULL AND (${alias}.branch_id IS NULL OR ${alias}.branch_id=?)))`, bindings: [...values, actor.departmentId, actor.branchId] }
        : { sql: `${base} AND (${alias}.branch_id IS NULL OR ?::text IS NULL OR ${alias}.branch_id=?)`, bindings: [...values, actor.branchId, actor.branchId] };
}
