import { nativePpQuery } from '@erpx/server-people_payroll/adapters/native-people-payroll-command';
import type { PoolClient } from 'pg';
export function lifecycleOperationsEffects(client: PoolClient) {
    return { applyEffect4(company_id: string, person_id: string) { return nativePpQuery<Record<string, unknown>>('pp_operations_people_lifecycle_worker_effects_stmt_1', [company_id, person_id]); },
        applyEffect19(company_id: string, person_id: string, value: unknown, value2: unknown) { return nativePpQuery<Record<string, unknown>>('pp_operations_people_lifecycle_worker_effects_stmt_2', [company_id, person_id, value, value2]); },
        applyEffect20(value: string, company_id: string, engagement_id: string, value2: string, value3: string, created_by: string, reason: string, id: string, now: number) { return nativePpQuery<Record<string, unknown>>('pp_operations_people_lifecycle_worker_effects_stmt_3', [value, company_id, engagement_id, value2, value3, created_by, reason, id, now]); } };
}
