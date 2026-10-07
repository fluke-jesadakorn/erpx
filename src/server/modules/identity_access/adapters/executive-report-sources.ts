import {identitySnapshotSources as organizationSnapshotSources} from '../../organization/public/identity-snapshot-sources';
import {identitySnapshotSources as peoplepayrollSnapshotSources} from '../../people_payroll/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
const peoplepayrollSources=peoplepayrollSnapshotSources();
import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { assertion } from "@/server/modules/workflow/public/commands";
import { type PreviewAssignmentRow } from '@/server/modules/runtime/public/executive/report-contract';
const targetGuard = `EXISTS(SELECT 1 FROM assignments g ${organizationSources.executive_report_sources_organization_nodes_c_join} ON c.id=g.company_id AND c.kind='company' AND c.status='active'
    ${peoplepayrollSources.executive_report_sources_hr_people_p_join} ON p.company_id=g.company_id AND p.user_id=g.user_id
    ${peoplepayrollSources.executive_report_sources_hr_engagements_e_join} ON e.company_id=p.company_id AND e.person_id=p.id ${organizationSources.executive_report_sources_staff_levels_l_join} ON l.company_id=e.company_id AND l.id=e.level_id
    WHERE g.id=? AND g.company_id=? AND g.user_id=? AND g.role=? AND g.version=? AND g.scope_kind=? AND g.scope_node_id IS NOT DISTINCT FROM ?
      AND g.revoked_at IS NULL AND g.valid_from<=? AND g.valid_until>? AND iam_assignment_active(g.id,?) AND iam_principal_active(g.user_id,g.company_id,?)
      AND (g.scope_kind='holding' OR EXISTS(SELECT 1 ${organizationSources.executive_report_sources_organization_nodes_n_from} WHERE n.id=g.scope_node_id AND n.company_id=g.company_id AND n.status='active'
        AND (n.branch_id IS NULL OR EXISTS(SELECT 1 ${organizationSources.executive_report_sources_organization_nodes_b_from} WHERE b.id=n.branch_id AND b.company_id=g.company_id AND b.status='active'))))
      AND l.active=1 AND l.level_number IN (1,2) AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')
      AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))
      AND COALESCE((SELECT m.version FROM iam_members m WHERE m.company_id=g.company_id AND m.user_id=g.user_id),0)=?
      AND EXISTS(SELECT 1 FROM iam_roles r JOIN iam_role_revisions rv ON rv.company_id=r.company_id AND rv.role_id=r.id AND rv.revision=r.published_revision
        WHERE r.id=g.role AND r.company_id IN ('*',g.company_id) AND r.status='active' AND r.published_revision=? AND rv.scopes_json @> jsonb_build_array(?::text)
          AND rv.permissions_json @> '["executive.access","executive.report.create","executive.report.edit","executive.report.share"]'::jsonb))`;
export function executiveReportSources(database: SqlDatabase) {
    return { readRecipientPreviewAssignmentsRows(companyId: string, recipientId: string, now: number, now2: number, now3: number, now4: number) {
            return database.prepare(`SELECT a.id,a.role,a.modules_json,a.company_id,a.branch_id,a.version,a.valid_until,a.scope_kind,a.scope_node_id,
      n.display_name scope_name,n.slug scope_slug,b.id scope_branch_id,b.display_name scope_branch_name,b.slug scope_branch_slug
    FROM assignments a ${organizationSources.executive_report_sources_organization_nodes_c_join} ON c.id=a.company_id AND c.kind='company' AND c.status='active'
    LEFT ${organizationSources.executive_report_sources_organization_nodes_n_join} ON n.id=a.scope_node_id AND n.company_id=a.company_id
    LEFT ${organizationSources.executive_report_sources_organization_nodes_b_join} ON b.id=CASE WHEN a.scope_kind='branch' THEN a.scope_node_id WHEN a.scope_kind='department' THEN COALESCE(a.branch_id,n.branch_id) ELSE NULL END AND b.kind='branch' AND b.company_id=a.company_id AND b.status='active'
    WHERE a.company_id=? AND a.user_id=? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND iam_assignment_active(a.id,?)
      AND iam_principal_active(a.user_id,a.company_id,?) AND (a.scope_kind='holding' OR (n.status='active' AND ((a.scope_kind='branch' AND n.kind='branch') OR (a.scope_kind='department' AND n.kind='department' AND (n.branch_id IS NULL OR n.branch_id=b.id)))))
    ORDER BY a.scope_kind,a.role,a.id`).bind(companyId, recipientId, now, now2, now3, now4).all<PreviewAssignmentRow>();
        }, readRecipientPreviewAssignmentsCurrentRows(companyId: string, recipientId: string, now: number) {
            return database.prepare("SELECT assignment_id FROM active_work_contexts WHERE company_id=? AND user_id=? AND expires_at>? AND assignment_id IS NOT NULL ORDER BY updated_at DESC").bind(companyId, recipientId, now).all<{
                assignment_id: string;
            }>();
        }, readRecipientPreviewContextActiveContext(companyId: string, recipientId: string, assignmentId: string, now: number) {
            return database.prepare("SELECT session_id,revision,assignment_version FROM active_work_contexts WHERE company_id=? AND user_id=? AND assignment_id=? AND expires_at>? ORDER BY updated_at DESC LIMIT 1").bind(companyId, recipientId, assignmentId, now).first<{
                session_id: string;
                revision: number;
                assignment_version: number;
            }>();
        }, readOwnershipCandidatesActiveContexts(companyId: string, now: number, value: string[], rows: (PreviewAssignmentRow & {
            user_id: string;
            legal_name: string;
        })[]) {
            return database.prepare(`SELECT assignment_id FROM active_work_contexts WHERE company_id=? AND expires_at>? AND assignment_id IN (${rows.map(() => "?").join(",")})`).bind(companyId, now, ...value).all<{
                assignment_id: string;
            }>();
        }, guard33(key: string, value: "transfer-target", id: string, companyId: string, userId: string, role: string, value2: number, assignmentScope: import("@/server/modules/identity_access/public/authorization").AssignmentScopeKind, scopeNodeId: string | null, now: number, now2: number, now3: number, now4: number, value3: number, value4: number, assignmentScope2: import("@/server/modules/identity_access/public/authorization").AssignmentScopeKind) { return assertion(database, key, value, targetGuard, [id, companyId, userId, role, value2, assignmentScope, scopeNodeId, now, now2, now3, now4, value3, value4, assignmentScope2]); } };
}
