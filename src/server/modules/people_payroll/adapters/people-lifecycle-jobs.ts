import { drainOrganizationDrafts } from "@/server/modules/organization/public/organization-drafts";
import "server-only";
import { iamDb } from "@/server/modules/identity_access/public/iam-db";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import type { LifecycleCase } from "@/lib/people_payroll/people-lifecycle";
import { drainPositionMoves } from "@/server/modules/organization/public/position-moves";
import type { PoolClient } from 'pg';
import { type LifecycleEngagementFacts } from '../domain/people-lifecycle-jobs';
import { createPeopleLifecycleWorker, type PeopleLifecycleWorkerRepository } from '../application/people-lifecycle-jobs';
import { lifecycleIdentityEffects } from '../../identity_access/public/people-lifecycle-worker-effects';
import { lifecycleOperationsEffects } from '../../operations/public/people-lifecycle-worker-effects';
import { lifecycleOrganizationFacts } from '../../organization/public/people-lifecycle-worker-effects';
import { lifecycleOutboxEffects } from '../../workflow/public/people-lifecycle-worker-effects';
function repository(client: PoolClient): PeopleLifecycleWorkerRepository {
    return { readAvailable() { return client.query<{
            available: boolean;
        }>("SELECT pg_try_advisory_xact_lock(821039320) AS available"); },
        readRow(now: number) { return client.query<LifecycleCase>("SELECT * FROM people_lifecycle_cases WHERE status='scheduled' AND effective_at<=$1 AND next_attempt_at<=$1 ORDER BY effective_at,id FOR UPDATE SKIP LOCKED LIMIT 1", [now]); },
        readPeople(person_id: string, company_id: string) { return client.query<{
            status: string;
        }>("SELECT * FROM hr_people WHERE id=$1 AND company_id=$2 FOR UPDATE", [person_id, company_id]); },
        readEngagement(engagement_id: string, person_id: string, company_id: string) { return client.query<LifecycleEngagementFacts>("SELECT * FROM hr_engagements WHERE id=$1 AND person_id=$2 AND company_id=$3 FOR UPDATE", [engagement_id, person_id, company_id]); },
        applyEffect(...args) { return lifecycleIdentityEffects(client).applyEffect(...args); },
        applyEffect2(...args) { return lifecycleIdentityEffects(client).applyEffect2(...args); },
        applyEffect3(engagement_id: string, value: "ended" | "suspended", lastWorkingDate: string | null, now: number) { return client.query<Record<string, unknown>>("UPDATE hr_engagements SET status=$2,end_date=CASE WHEN $2='ended' THEN $3 ELSE end_date END,version=version+1,updated_at=$4 WHERE id=$1", [engagement_id, value, lastWorkingDate, now]); },
        applyEffect4(...args) { return lifecycleOperationsEffects(client).applyEffect4(...args); },
        applyEffect5(id: string, now: number) { return client.query<{
            ok: boolean;
        }>("SELECT people_case_active($1,$2) ok", [id, now]); },
        readValid(...args) { return lifecycleIdentityEffects(client).readValid(...args); },
        readSaved(id: string) { return client.query<{
            review_snapshot: unknown;
        }>("SELECT review_snapshot FROM people_lifecycle_cases WHERE id=$1", [id]); },
        readPosition(...args) { return lifecycleOrganizationFacts(client).readPosition(...args); },
        applyEffect6(...args) { return lifecycleOrganizationFacts(client).applyEffect6(...args); },
        applyEffect7(...args) { return lifecycleIdentityEffects(client).applyEffect7(...args); },
        applyEffect8(...args) { return lifecycleIdentityEffects(client).applyEffect8(...args); },
        applyEffect9(company_id: string, person_id: string, effective_at: number) { return client.query<Record<string, unknown>>("UPDATE people_access_cutoffs SET restored_at=$3 WHERE company_id=$1 AND person_id=$2 AND cancelled_at IS NULL AND restored_at IS NULL AND effective_at<=$3", [company_id, person_id, effective_at]); },
        applyEffect10(company_id: string, person_id: string, effective_at: number) { return client.query<Record<string, unknown>>("UPDATE people_applied_packages SET ended_at=$3 WHERE company_id=$1 AND person_id=$2 AND source_kind='position' AND ended_at IS NULL", [company_id, person_id, effective_at]); },
        applyEffect11(...args) { return lifecycleIdentityEffects(client).applyEffect11(...args); },
        applyEffect12(id: string) { return client.query<Record<string, unknown>>("UPDATE people_lifecycle_cases SET status='completed' WHERE id=$1", [id]); },
        applyEffect13(value: string) { return client.query<Record<string, unknown>>("SELECT set_config('erpx.payroll_assignment_date',$1,true)", [value]); },
        applyEffect14(engagement_id: string, id: unknown, value: unknown, value2: unknown, level_id: unknown, manager: string | null, title: unknown, now: number) { return client.query<Record<string, unknown>>("UPDATE hr_engagements SET position_id=$2,branch_id=$3,department_id=$4,level_id=$5,manager_person_id=$6,job_title=$7,status='active',version=version+1,updated_at=$8 WHERE id=$1", [engagement_id, id, value, value2, level_id, manager, title, now]); },
        readPay(...args) { return lifecycleOrganizationFacts(client).readPay(...args); },
        readExisting(engagement_id: string) { return client.query<Record<string, unknown>>("SELECT 1 FROM hr_compensation_revisions WHERE engagement_id=$1 LIMIT 1", [engagement_id]); },
        applyEffect15(engagement_id: string, pay_group_id: unknown) { return client.query<Record<string, unknown>>("UPDATE hr_engagements SET pay_group_id=$2 WHERE id=$1", [engagement_id, pay_group_id]); },
        applyEffect16(value: string, engagement_id: string, value2: string, monthly_amount_minor: unknown, currency: unknown, created_by: string, now: number) { return client.query<Record<string, unknown>>("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,created_by,created_at,updated_at) VALUES($1,$2,$3,'monthly',$4,0,0,$5,'[]','draft',$6,$7,$7) ON CONFLICT(id) DO NOTHING", [value, engagement_id, value2, monthly_amount_minor, currency, created_by, now]); },
        readPolicy(...args) { return lifecycleOrganizationFacts(client).readPolicy(...args); },
        readPrior(company_id: string, engagement_id: string, date: string) { return client.query<{
            id: string;
            effective_from: string;
            effective_to: string | null;
            policy_key: string;
        }>("SELECT a.id,a.effective_from,a.effective_to,p.policy_key FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id WHERE a.company_id=$1 AND a.engagement_id=$2 AND (a.effective_to IS NULL OR a.effective_to>=$3) ORDER BY a.effective_from DESC LIMIT 1", [company_id, engagement_id, date]); },
        applyEffect17(until: string, id: unknown) { return client.query<Record<string, unknown>>("UPDATE leave_policy_assignments SET effective_to=$1,version=version+1 WHERE id=$2", [until, id]); },
        applyEffect18(value: string, company_id: string, engagement_id: string, policy_version_id: unknown, effective: unknown, created_by: string, now: number) { return client.query<Record<string, unknown>>("INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,effective_from,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(id) DO NOTHING", [value, company_id, engagement_id, policy_version_id, effective, created_by, now]); },
        applyEffect19(...args) { return lifecycleOperationsEffects(client).applyEffect19(...args); },
        applyEffect20(...args) { return lifecycleOperationsEffects(client).applyEffect20(...args); },
        applyEffect21(id: string, now: number) { return client.query<Record<string, unknown>>("UPDATE people_lifecycle_cases SET status='completed',version=version+1,updated_at=$2 WHERE id=$1", [id, now]); },
        applyEffect22(...args) { return lifecycleIdentityEffects(client).applyEffect22(...args); },
        applyEffect23(...args) { return lifecycleOutboxEffects(client).applyEffect23(...args); },
        applyEffect24(currentId: string, now: number, code: string) { return client.query<Record<string, unknown>>("UPDATE people_lifecycle_cases SET attempts=attempts+1,next_attempt_at=$2+LEAST(3600000,1000*power(2,LEAST(attempts,12)))::bigint,last_error=$3 WHERE id=$1 AND status='scheduled'", [currentId, now, code]); } };
}
export async function drainPeopleLifecycle(raw: PostgresDatabase, now = Date.now()) { return createPeopleLifecycleWorker({ withSession: async (run) => { const client = await iamDb(raw).pool.connect(); try {
        return await run({ repository: repository(client), transaction: async (run) => { await client.query('BEGIN ISOLATION LEVEL SERIALIZABLE'); try {
                const result = await run({ repository: repository(client) });
                await client.query('COMMIT');
                return result;
            }
            catch (error) {
                await client.query('ROLLBACK');
                throw error;
            } } });
    }
    finally {
        client.release();
    } }, drainPositionMoves: time => drainPositionMoves(raw, time), drainOrganizationDrafts: time => drainOrganizationDrafts(raw, time) }).drainPeopleLifecycle(now); }
