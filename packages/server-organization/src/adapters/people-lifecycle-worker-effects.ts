import { platformOwnerProjectionSources as people_payrollPlatformSources } from "@erpx/server-people_payroll/public/platform-owner-projection-sources";
import { leavePolicyVersionLookup } from './organization-column-alias-families-r37';
import { activePositionById } from './organization-column-alias-families-r38';
import type { PoolClient } from 'pg';
import type { LifecyclePositionFacts } from '@erpx/server-people_payroll/public/lifecycle-worker-contract';
import {personEngagementJoin} from './organization-read-tail-sources-r55';
export function lifecycleOrganizationFacts(client: PoolClient) {
    return { readPosition(company_id: string, positionId: string | null) { return client.query<LifecyclePositionFacts>(activePositionById('$1', '$2'), [company_id, positionId]); },
        applyEffect6(manager: string, company_id: string, level_id: unknown) { return client.query<Record<string, unknown>>(`${personEngagementJoin("JOIN staff_levels l ON l.id=e.level_id JOIN staff_levels target ON target.id=$3 WHERE p.id=$1 AND p.company_id=$2 AND p.status='active' AND e.status='active' AND l.level_number<target.level_number")}`, [manager, company_id, level_id]); },
        readPay(company_id: string, id: unknown) { return client.query<{
            pay_group_id: string;
            currency: string;
            monthly_amount_minor: number;
        }>("SELECT pay_group_id,currency,monthly_amount_minor FROM position_pay_defaults WHERE company_id=$1 AND position_id=$2 ORDER BY version DESC LIMIT 1", [company_id, id]); },
        readPolicy(company_id: string, level_id: unknown) { return client.query<{
            policy_version_id: string;
            effective_from: string;
        }>(leavePolicyVersionLookup(people_payrollPlatformSources.leave_policy_versions,'$1','$2'), [company_id, level_id]); } };
}
