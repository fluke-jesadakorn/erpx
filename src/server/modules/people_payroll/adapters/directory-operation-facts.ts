import {peoplePayrollFinalOwnerProjections as identityaccessFinalOwnerProjections} from "../../identity_access/public/people-payroll-final-owner-projections";
const identityaccessFinalSources=identityaccessFinalOwnerProjections();
import 'server-only';
import type { PoolClient, Pool } from 'pg';
import type { OperationTransaction, DirectoryOperationPorts } from '@/server/modules/identity_access/public/directory-operation-contract';
export async function lifecycle(c: Pick<PoolClient | Pool, 'query'>, facts: Parameters<OperationTransaction['lifecycle']>[0]): ReturnType<OperationTransaction['lifecycle']> {
    const { action, actor, observedNow } = facts;
    return (await c.query("SELECT c.*,(people_case_active(c.id,$3) OR c.workflow='placement' AND c.status='completed' AND c.effective_at<=$3 AND EXISTS(SELECT 1 FROM people_applied_packages ap WHERE ap.company_id=c.company_id AND ap.person_id=c.person_id AND people_application_active(ap.id,$3))) active FROM people_lifecycle_cases c WHERE c.id=$1 AND c.company_id=$2", [action.caseId, actor.companyId, observedNow])).rows[0] ?? null;
}
export async function currentPlacement(c: Pick<PoolClient | Pool, 'query'>, facts: Parameters<OperationTransaction['currentPlacement']>[0]): ReturnType<OperationTransaction['currentPlacement']> {
    const { plan, job, observedNow, op } = facts;
    return (await c.query("SELECT 1 FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE e.id=$1 AND e.company_id=$2 AND e.version=$3 AND ($6::boolean OR e.status='active' AND p.status='active') AND p.merged_into_id IS NULL AND ($6::boolean OR NOT EXISTS(SELECT 1 FROM people_access_cutoffs x WHERE x.company_id=e.company_id AND x.person_id=e.person_id AND x.cancelled_at IS NULL AND x.restored_at IS NULL AND x.effective_at<=$4 AND NOT ($5::boolean AND EXISTS(SELECT 1 FROM people_lifecycle_cases lc WHERE lc.engagement_id=e.id AND lc.workflow='placement' AND lc.status='completed' AND lc.kind IN ('onboarding','transfer') AND lc.effective_at>x.effective_at))))", [
        plan.id,
        job.company_id,
        plan.engagementVersion,
        observedNow,
        op.definition.restoreAccess === true,
        op.definition.revokeOnly === true,
    ])).rowCount;
}
export async function provisioning(c: Pick<PoolClient | Pool, 'query'>, facts: Parameters<OperationTransaction['provisioning']>[0]): ReturnType<OperationTransaction['provisioning']> {
    const { job } = facts;
    return (await c.query("SELECT * FROM people_provision_operations WHERE id=$1", [job.provision_id])).rows[0] ?? null;
}
export async function provisioningSources(c: Pick<PoolClient | Pool, 'query'>, facts: Parameters<OperationTransaction['provisioningSources']>[0]): ReturnType<OperationTransaction['provisioningSources']> { const { job } = facts; return (await c.query("SELECT evidence FROM people_provision_operations WHERE id=$1", [job.provision_id])).rows[0] ?? null; }
export async function completionProvisioning(c: Pick<PoolClient | Pool, 'query'>, facts: Parameters<OperationTransaction['completionProvisioning']>[0]): ReturnType<OperationTransaction['completionProvisioning']> {
    const { job } = facts;
    return (await c.query("SELECT evidence FROM people_provision_operations WHERE id=$1", [job.provision_id])).rows[0] ?? null;
}
export async function resumeProvisioning(c: Pick<PoolClient | Pool, 'query'>, facts: Parameters<OperationTransaction['resumeProvisioning']>[0]): ReturnType<OperationTransaction['resumeProvisioning']> {
    const { row, actor } = facts;
    return void await c.query("UPDATE people_provision_operations SET status='queued',session_id=$2,last_error=NULL WHERE id=$1 AND status IN ('blocked','running')", [row.provision_id, actor.sessionId]);
}
export async function affectedPeople(c: Pick<PoolClient | Pool, 'query'>, facts: Parameters<OperationTransaction['affectedPeople']>[0]): ReturnType<OperationTransaction['affectedPeople']> {
    const { identities, positionRows } = facts;
    return (await c.query(`SELECT DISTINCT p.legal_name FROM hr_people p LEFT ${identityaccessFinalSources.directory_operation_facts_iam_members_m_join} ON m.person_id=p.id AND m.company_id=p.company_id WHERE p.user_id=ANY($1::text[]) OR m.user_id=ANY($1::text[]) OR EXISTS(SELECT 1 FROM hr_engagements e WHERE e.person_id=p.id AND e.company_id=p.company_id AND e.position_id=ANY($2::text[]) AND e.status='active') ORDER BY p.legal_name`, [
        identities.map((i) => i.user_id).filter(Boolean),
        positionRows.map((p) => p.id),
    ])).rows;
}
export async function lifecycleCases(c: Pick<PoolClient | Pool, 'query'>, facts: Parameters<DirectoryOperationPorts['lifecycleCases']>[0]): ReturnType<DirectoryOperationPorts['lifecycleCases']> {
    const { actor } = facts;
    return (await c.query(`SELECT c.id,p.legal_name name,c.kind,c.effective_at,ARRAY(SELECT i.external_id ${identityaccessFinalSources.directory_operation_facts_iam_identities_i_from} ${identityaccessFinalSources.directory_operation_facts_iam_members_m_join} ON m.identity_id=i.id AND m.user_id=i.user_id AND m.company_id=i.company_id WHERE m.person_id=c.person_id AND m.company_id=c.company_id AND m.status='active') account_ids FROM people_lifecycle_cases c JOIN hr_people p ON p.id=c.person_id AND p.company_id=c.company_id WHERE c.company_id=$1 AND c.status IN ('scheduled','completed') ORDER BY c.updated_at DESC LIMIT 200`, [actor.companyId])).rows;
}
