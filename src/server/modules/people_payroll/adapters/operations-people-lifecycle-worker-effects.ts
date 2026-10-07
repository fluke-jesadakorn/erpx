import type { PoolClient } from 'pg';
export function lifecycleOperationsEffects(client: PoolClient) {
    return { applyEffect4(company_id: string, person_id: string) { return client.query<Record<string, unknown>>("UPDATE employees SET status='inactive' WHERE company_id=$1 AND person_id=$2", [company_id, person_id]); },
        applyEffect19(company_id: string, person_id: string, value: unknown, value2: unknown) { return client.query<Record<string, unknown>>("UPDATE employees SET branch_id=$3,department_id=$4,status='active' WHERE company_id=$1 AND person_id=$2", [company_id, person_id, value, value2]); },
        applyEffect20(value: string, company_id: string, engagement_id: string, value2: string, value3: string, created_by: string, reason: string, id: string, now: number) { return client.query<Record<string, unknown>>("INSERT INTO staff_organization_history(id,company_id,engagement_id,before_json,after_json,actor_id,reason,command_id,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)", [value, company_id, engagement_id, value2, value3, created_by, reason, id, now]); } };
}
