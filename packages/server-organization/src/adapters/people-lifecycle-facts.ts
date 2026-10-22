import { platformOwnerProjectionSources as people_payrollPlatformSources } from "@erpx/server-people_payroll/public/platform-owner-projection-sources";
import "server-only";
import type { PoolClient } from "pg";
import { type PeopleLifecycleData } from "@erpx/feature-people_payroll/lib/people-lifecycle";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { type LifecyclePositionFact } from '@erpx/server-people_payroll/public/people-lifecycle-contract';
export function peopleLifecycleOrganizationFacts(database: SqlDatabase, client?: PoolClient) {
    return { readPeopleLifecycleDataPositions(companyId: string) {
            return database.prepare(`SELECT p.id,p.title,p.version,p.requirements,p.branch_id,p.department_id,p.level_id,p.active,b.package_id,b.revision package_revision FROM organization_positions p LEFT JOIN (${people_payrollPlatformSources.people_position_packages}) b ON b.company_id=p.company_id AND b.position_id=p.id WHERE p.company_id=? ORDER BY p.title,p.id`)
                .bind(companyId)
                .all<PeopleLifecycleData["positions"][number]>();
        },
        readPeopleLifecycleDataNodes(companyId: string) {
            return database.prepare("SELECT id,display_name name,kind FROM organization_nodes WHERE company_id=? AND status='active' ORDER BY display_name,id")
                .bind(companyId)
                .all<{
                id: string;
                name: string;
                kind: string;
            }>();
        },
        readPackageImpactPositions(companyId: string, id: string) {
            return database.prepare(`SELECT p.id,p.title,p.version FROM (${people_payrollPlatformSources.people_position_packages}) b JOIN organization_positions p ON p.id=b.position_id AND p.company_id=b.company_id WHERE b.company_id=? AND b.package_id=? ORDER BY p.id`)
                .bind(companyId, id)
                .all<{
                id: string;
                title: string;
                version: number;
            }>();
        },
        readPreviewLifecyclePosition(companyId: string, positionId: string) {
            return database.prepare(`SELECT p.*,b.package_id,b.revision FROM organization_positions p LEFT JOIN (${people_payrollPlatformSources.people_position_packages}) b ON b.position_id=p.id AND b.company_id=p.company_id WHERE p.company_id=? AND p.id=? AND p.active=1`)
                .bind(companyId, positionId)
                .first<{
                id: string;
                branch_id: string | null;
                department_id: string | null;
                level_id: string;
                package_id: string | null;
                revision: number | null;
            }>();
        },
        readPreviewLifecycleRecommendedPay(companyId: string, id: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) { return database.prepare("SELECT d.currency,d.monthly_amount_minor FROM position_pay_defaults d JOIN organization_positions p ON p.id=d.position_id WHERE d.company_id=? AND d.position_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?) ORDER BY d.version DESC LIMIT 1").bind(companyId, id, branchId, branchId2, departmentId, departmentId2).first<{
            currency: string;
            monthly_amount_minor: number;
        }>(); },
        readPreviewLifecycleRecommendedLeave(companyId: string, level_id: string) {
            return database.prepare(`SELECT l.name level_name,max(CASE WHEN t.code='annual' THEN r.grant_units END) annual_days,max(CASE WHEN t.code='sick' THEN r.grant_units END) sick_days
    FROM level_leave_defaults d JOIN staff_levels l ON l.id=d.level_id JOIN (${people_payrollPlatformSources.leave_policy_rules}) r ON r.policy_version_id=d.policy_version_id JOIN (${people_payrollPlatformSources.leave_types}) t ON t.id=r.leave_type_id
    WHERE d.company_id=? AND d.level_id=? AND d.version=(SELECT max(x.version) FROM level_leave_defaults x WHERE x.company_id=d.company_id AND x.level_id=d.level_id) GROUP BY l.name`)
                .bind(companyId, level_id).first<{
                level_name: string;
                annual_days: number;
                sick_days: number;
            }>();
        },
        readSaveLifecyclePosition(companyId: string, positionId: string) {
            return database.prepare("SELECT level_id,branch_id,department_id FROM organization_positions WHERE company_id=? AND id=?")
                .bind(companyId, positionId)
                .first<{
                level_id: string;
                branch_id: string | null;
                department_id: string | null;
            }>();
        },
        readCreateRehireEngagementPosition(companyId: string, positionId: string) {
            return database.prepare("SELECT * FROM organization_positions WHERE company_id=? AND id=? AND active=1")
                .bind(companyId, positionId)
                .first<{
                branch_id: string | null;
                department_id: string | null;
                level_id: string;
                title: string;
            }>();
        },
        applyValidatePackageEffect2(companyId: unknown, scopeNodeId: unknown, scopeKind: unknown) { return client!.query<Record<string, unknown>>("SELECT 1 FROM organization_nodes WHERE company_id=$1 AND id=$2 AND kind=$3 AND status='active'", [companyId, scopeNodeId, scopeKind]); },
        readBindPositionPackageposition(companyId: unknown, positionId: unknown) { return client!.query<LifecyclePositionFact>("SELECT * FROM organization_positions WHERE company_id=$1 AND id=$2 AND active=1 FOR UPDATE", [companyId, positionId]); },
        applyBindPositionPackageEffect2(companyId: unknown, id: unknown) { return client!.query<Record<string, unknown>>("UPDATE organization_positions SET version=version+1 WHERE company_id=$1 AND id=$2", [companyId, id]); } };
}
