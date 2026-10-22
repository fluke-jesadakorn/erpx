import {rosterCoverageGapSql} from '@erpx/server-organization/adapters/roster-source-scope';
import { platformOwnerProjectionSources as identity_accessPlatformSources } from "@erpx/server-identity_access/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as people_payrollPlatformSources } from "@erpx/server-people_payroll/public/platform-owner-projection-sources";
import "server-only";
import { type RosterDepartment } from "@erpx/feature-people_payroll/lib/people-roster";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { RosterGap } from '@erpx/server-people_payroll/public/roster-worker-contract';
export function rosterOrganizationFacts(database: SqlDatabase) {
    return { async readDepartments(fact0: string | null, fact1: string, fact2: string, fact3: string, fact4: string | null, fact5: string | null, fact6: string | null, fact7: boolean, fact8: string | null) {
            return (await database.prepare(`WITH RECURSIVE subtree AS (
    SELECT id FROM organization_nodes WHERE id=? AND company_id=?
    UNION ALL SELECT n.id FROM organization_nodes n JOIN subtree p ON n.parent_id=p.id WHERE n.company_id=? AND n.status='active')
    SELECT d.id,d.display_name name,d.branch_id,b.display_name branch_name FROM organization_nodes d
    LEFT JOIN organization_nodes b ON b.id=d.branch_id
    WHERE d.company_id=? AND d.kind='department' AND d.status='active'
    AND (?::text IS NULL OR d.branch_id=?) AND (?::text IS NULL OR d.id IN (SELECT id FROM subtree))
    AND (? OR d.id=?) ORDER BY b.display_name,d.display_name`).bind(fact0, fact1, fact2, fact3, fact4, fact5, fact6, fact7, fact8).all<RosterDepartment>()).results;
        },
        async readRosterGaps(fact0: string, fact1: string, fact2: string, fact3: string, fact4: string, fact5: string, fact6: string, fact7: string) {
            return (await database.prepare(rosterCoverageGapSql('parameters')).bind(fact0, fact1, fact2, fact3, fact4, fact5, fact6, fact7).all<RosterGap>()).results;
        },
        async readEscalateRosterCoverage() {
            return (await database.prepare(`
    SELECT DISTINCT e.company_id,e.department_id,d.branch_id FROM (${people_payrollPlatformSources.hr_engagements}) e
    JOIN organization_positions p ON p.id=e.position_id AND p.company_id=e.company_id AND p.active=1 AND p.roster_required=1
    JOIN organization_nodes d ON d.id=e.department_id AND d.company_id=e.company_id AND d.kind='department' AND d.status='active'
    WHERE e.status='active'`).all<{
                company_id: string;
                department_id: string;
                branch_id: string | null;
            }>()).results;
        },
        async readNotifyManagers(fact0: string, fact1: string, fact2: string, fact3: string, fact4: number, fact5: number, fact6: string | null, fact7: string) {
            return (await database.prepare(`WITH RECURSIVE ancestors AS (
    SELECT id,parent_id FROM organization_nodes WHERE id=? AND company_id=?
    UNION ALL SELECT n.id,n.parent_id FROM organization_nodes n JOIN ancestors c ON n.id=c.parent_id WHERE n.company_id=? AND n.status='active')
    SELECT DISTINCT a.user_id FROM (${identity_accessPlatformSources.assignments}) a WHERE a.company_id=? AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?) AND iam_role_allows(a.role,a.company_id,'roster.approve')
    AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=? OR a.scope_kind='department' AND a.scope_node_id IN (SELECT id FROM ancestors))
    AND NOT EXISTS(SELECT 1 FROM (${people_payrollPlatformSources.hr_people}) p WHERE p.id=ANY(?::text[]) AND iam_same_person(p.user_id,a.user_id,p.company_id))`).bind(fact0, fact1, fact2, fact3, fact4, fact5, fact6, fact7).all<{
                user_id: string;
            }>()).results;
        } };
}
