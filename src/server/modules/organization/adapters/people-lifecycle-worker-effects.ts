import { platformOwnerProjectionSources as people_payrollPlatformSources } from "../../people_payroll/public/platform-owner-projection-sources";
import type { PoolClient } from 'pg';
import type { LifecyclePositionFacts } from '../../people_payroll/public/lifecycle-worker-contract';
export function lifecycleOrganizationFacts(client: PoolClient) {
    return { readPosition(company_id: string, positionId: string | null) { return client.query<LifecyclePositionFacts>("SELECT * FROM organization_positions WHERE company_id=$1 AND id=$2 AND active=1", [company_id, positionId]); },
        applyEffect6(manager: string, company_id: string, level_id: unknown) { return client.query<Record<string, unknown>>(`SELECT 1 FROM (${people_payrollPlatformSources.hr_people}) p JOIN (${people_payrollPlatformSources.hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id JOIN staff_levels l ON l.id=e.level_id JOIN staff_levels target ON target.id=$3 WHERE p.id=$1 AND p.company_id=$2 AND p.status='active' AND e.status='active' AND l.level_number<target.level_number`, [manager, company_id, level_id]); },
        readPay(company_id: string, id: unknown) { return client.query<{
            pay_group_id: string;
            currency: string;
            monthly_amount_minor: number;
        }>("SELECT pay_group_id,currency,monthly_amount_minor FROM position_pay_defaults WHERE company_id=$1 AND position_id=$2 ORDER BY version DESC LIMIT 1", [company_id, id]); },
        readPolicy(company_id: string, level_id: unknown) { return client.query<{
            policy_version_id: string;
            effective_from: string;
        }>(`SELECT d.policy_version_id,p.effective_from FROM level_leave_defaults d JOIN (${people_payrollPlatformSources.leave_policy_versions}) p ON p.id=d.policy_version_id WHERE d.company_id=$1 AND d.level_id=$2 ORDER BY d.version DESC LIMIT 1`, [company_id, level_id]); } };
}
