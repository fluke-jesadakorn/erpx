import {peoplePayrollFinalOwnerProjections as identityaccessFinalOwnerProjections} from "../../identity_access/public/people-payroll-final-owner-projections";
const identityaccessFinalSources=identityaccessFinalOwnerProjections();
import {peoplePayrollFinalOwnerProjections as organizationFinalOwnerProjections} from "../../organization/public/people-payroll-final-owner-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { assertion } from "@/server/modules/workflow/public/commands";
import { type PreviewAssignmentRow } from '@/server/modules/runtime/public/executive/report-contract';
export function executiveReportSources(database: SqlDatabase) {
    return { readReportAccessEffect(companyId: string, userId: string) { return database.prepare("SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=? AND p.user_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) LIMIT 1").bind(companyId, userId).first(); }, readReportLibraryAccessSetup(now: number, now2: number, now3: number, companyId: string) {
            return database.prepare(`SELECT DISTINCT p.id,p.legal_name name,l.level_number level,
    EXISTS(SELECT 1 ${identityaccessFinalSources.executive_report_sources_assignments_a_from} WHERE a.company_id=p.company_id AND a.user_id=p.user_id AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND iam_assignment_active(a.id,?) AND iam_role_allows(a.role,a.company_id,'executive.access') AND iam_role_allows(a.role,a.company_id,'executive.report.create')) ready
    FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id ${organizationFinalSources.executive_report_sources_staff_levels_l_join} ON l.id=e.level_id AND l.company_id=e.company_id
    WHERE p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND l.active=1 AND l.level_number IN (1,2)
    AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) ORDER BY l.level_number,p.legal_name LIMIT 100`).bind(now, now2, now3, companyId).all<{
                id: string;
                name: string;
                level: number;
                ready: boolean;
            }>();
        }, readRecipientsEffect(companyId: string, userId: string, q: string, now: number, now2: number, now3: number, now4: number) {
            return database.prepare(`SELECT DISTINCT p.user_id id,p.legal_name name FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=? AND p.user_id IS NOT NULL AND p.user_id<>? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) AND strpos(lower(p.legal_name),lower(?))>0 AND iam_principal_active(p.user_id,p.company_id,?) AND EXISTS(SELECT 1 ${identityaccessFinalSources.executive_report_sources_assignments_a_from} WHERE a.company_id=p.company_id AND a.user_id=p.user_id AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND iam_assignment_active(a.id,?)) ORDER BY p.legal_name LIMIT 40`).bind(companyId, userId, q, now, now2, now3, now4).all<{
                id: string;
                name: string;
            }>();
        }, readActiveShareRecipientEffect(companyId: string, recipientId: string, now: number, now2: number, now3: number, now4: number) {
            return database.prepare(`SELECT p.legal_name FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
    WHERE p.company_id=? AND p.user_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active'
      AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))
      AND EXISTS(SELECT 1 ${identityaccessFinalSources.executive_report_sources_assignments_r_from} WHERE r.company_id=p.company_id AND r.user_id=p.user_id AND r.revoked_at IS NULL AND r.valid_from<=? AND r.valid_until>?
        AND iam_assignment_active(r.id,?) AND iam_principal_active(r.user_id,r.company_id,?)) LIMIT 1`).bind(companyId, recipientId, now, now2, now3, now4).first<{
                legal_name: string;
            }>();
        }, guard21(key: string, value: "share-recipient", recipientId: string, companyId: string, now: number, companyId2: string, recipientId2: string, now2: number, now3: number, now4: number) { return assertion(database, key, value, `iam_principal_active(?,?,?) AND EXISTS(SELECT 1 ${identityaccessFinalSources.executive_report_sources_assignments_from} WHERE company_id=? AND user_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>? AND iam_assignment_active(id,?))`, [recipientId, companyId, now, companyId2, recipientId2, now2, now3, now4]); }, readRecipientPreviewAssignmentsEffect(companyId: string, recipientId: string, now: number) {
            return database.prepare(`SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
    WHERE p.company_id=? AND p.user_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active'
      AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))
      AND iam_principal_active(p.user_id,p.company_id,?) LIMIT 1`).bind(companyId, recipientId, now).first();
        }, readRecipientPreviewContextRecipient(companyId: string, recipientId: string) {
            return database.prepare("SELECT legal_name FROM hr_people WHERE company_id=? AND user_id=? AND status='active' AND merged_into_id IS NULL").bind(companyId, recipientId).first<{
                legal_name: string;
            }>();
        }, readOwnershipCandidatesRows(companyId: string, userId: string, now: number, now2: number, now3: number, now4: number) {
            return database.prepare(`SELECT a.id,a.role,a.modules_json,a.company_id,a.branch_id,a.version,a.valid_until,a.scope_kind,a.scope_node_id,
      p.user_id,n.display_name scope_name,n.slug scope_slug,b.id scope_branch_id,b.display_name scope_branch_name,b.slug scope_branch_slug,p.legal_name
    FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
    ${organizationFinalSources.executive_report_sources_staff_levels_l_join} ON l.id=e.level_id AND l.company_id=e.company_id AND l.active=1 AND l.level_number IN (1,2)
    ${identityaccessFinalSources.executive_report_sources_assignments_a_join} ON a.company_id=p.company_id AND a.user_id=p.user_id
    ${organizationFinalSources.executive_report_sources_organization_nodes_c_join} ON c.id=a.company_id AND c.kind='company' AND c.status='active'
    LEFT ${organizationFinalSources.executive_report_sources_organization_nodes_n_join} ON n.id=a.scope_node_id AND n.company_id=a.company_id
    LEFT ${organizationFinalSources.executive_report_sources_organization_nodes_b_join} ON b.id=CASE WHEN a.scope_kind='branch' THEN a.scope_node_id WHEN a.scope_kind='department' THEN COALESCE(a.branch_id,n.branch_id) ELSE NULL END AND b.kind='branch' AND b.company_id=a.company_id AND b.status='active'
    WHERE p.company_id=? AND p.user_id IS NOT NULL AND p.user_id<>? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active'
      AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))
      AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)
      AND EXISTS(SELECT 1 ${identityaccessFinalSources.executive_report_sources_iam_roles_r_from} ${identityaccessFinalSources.executive_report_sources_iam_role_revisions_rv_join} ON rv.company_id=r.company_id AND rv.role_id=r.id AND rv.revision=r.published_revision
        WHERE r.id=a.role AND r.company_id IN ('*',a.company_id) AND r.status='active'
          AND rv.permissions_json @> '["executive.access","executive.report.create","executive.report.edit","executive.report.share"]'::jsonb)
      AND (a.scope_kind='holding' OR (n.status='active' AND ((a.scope_kind='branch' AND n.kind='branch') OR (a.scope_kind='department' AND n.kind='department' AND (n.branch_id IS NULL OR n.branch_id=b.id)))))
    ORDER BY p.legal_name,a.scope_kind,a.role,a.id LIMIT 500`).bind(companyId, userId, now, now2, now3, now4).all<PreviewAssignmentRow & {
                user_id: string;
                legal_name: string;
            }>();
        } };
}
