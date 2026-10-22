import {stagePayrollAdvanceNotification,stagePayrollCorrectionNotification} from "@erpx/server-collaboration/public/identity-notification-effects";
import {identitySnapshotSources as peoplepayrollSnapshotSources} from '@erpx/server-people_payroll/public/identity-snapshot-sources';
const peoplepayrollSources=peoplepayrollSnapshotSources();
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
export function payrollIdentityFacts(database: SqlDatabase) {
    return { stageTransitionEngagementEffect3(now: number, user_id: string, companyId: string, person_id: string) { return database.prepare(`UPDATE assignments SET revoked_at=?,version=version+1 WHERE user_id=? AND company_id=? AND revoked_at IS NULL AND NOT EXISTS(SELECT 1 ${peoplepayrollSources.payroll_owner_facts_hr_engagements_from} WHERE person_id=? AND status='active')`).bind(now, user_id, companyId, person_id); },
        stageRequestPayrollAdvanceEffect3(id:string,companyId:string,id2:string,now:number,companyId2:string,now2:number,userId:string) {return stagePayrollAdvanceNotification(database,id,companyId,id2,now,companyId2,now2,userId);},
        readPayrollAdvanceAlertTargetEffect(advanceId: string, companyId: string, userId: string, userId2: string, now: number) {
            return database.prepare(`SELECT a.id ${peoplepayrollSources.payroll_owner_facts_hr_payroll_advances_a_from} ${peoplepayrollSources.payroll_owner_facts_hr_people_p_join} ON p.id=a.person_id
    WHERE a.id=? AND a.company_id=? AND (p.user_id=? OR EXISTS(
      SELECT 1 FROM assignments x WHERE x.company_id=a.company_id AND x.user_id=? AND x.scope_kind='holding' AND x.revoked_at IS NULL
      AND iam_assignment_active(x.id,?) AND iam_role_allows(x.role,x.company_id,'payroll.read')
    )) LIMIT 1`).bind(advanceId, companyId, userId, userId2, now).first<string>("id");
        },
        guard141(key: string, value: "mapping-review", mappingId: string, companyId: string, expectedVersion: number, userId: string) { return assertion(database, key, value, `EXISTS(SELECT 1 ${peoplepayrollSources.payroll_owner_facts_hr_payroll_account_mappings_from} WHERE id=? AND company_id=? AND status='draft' AND version=? AND NOT iam_same_person(created_by,?,company_id))`, [mappingId, companyId, expectedVersion, userId]); },
        guard155(key: string, value: "run", runId: string, companyId: string, expectedVersion: number, userId: string) { return assertion(database, key, value, `EXISTS(SELECT 1 ${peoplepayrollSources.payroll_owner_facts_hr_payroll_runs_from} WHERE id=? AND company_id=? AND version=? AND status='in_review' AND submitted_by IS NOT NULL AND NOT iam_same_person(submitted_by,?,company_id) AND submitted_digest=input_digest AND input_digest IS NOT NULL)`, [runId, companyId, expectedVersion, userId]); },
        stageRequestPayrollCorrectionEffect3(id:string,companyId:string,id2:string,now:number,companyId2:string,now2:number,userId:string) {return stagePayrollCorrectionNotification(database,id,companyId,id2,now,companyId2,now2,userId);},
        readPayrollCorrectionAlertTargetEffect(userId: string, correctionId: string, companyId: string, now: number) {
            return database.prepare(`SELECT c.run_id ${peoplepayrollSources.payroll_owner_facts_hr_payroll_corrections_c_from} JOIN assignments a ON a.company_id=c.company_id AND a.user_id=?
    WHERE c.id=? AND c.company_id=? AND a.scope_kind='holding' AND a.revoked_at IS NULL AND iam_assignment_active(a.id,?) AND iam_role_allows(a.role,a.company_id,'payroll.read') LIMIT 1`).bind(userId, correctionId, companyId, now).first<string>("run_id");
        },
        guard195(commandId: string, value: "batch-version", batchId: string, companyId: string, expectedVersion: number, userId: string) { return assertion(database, commandId, value, `EXISTS(SELECT 1 ${peoplepayrollSources.payroll_owner_facts_hr_payment_batches_from} WHERE id=? AND company_id=? AND version=? AND status='draft' AND NOT iam_same_person(COALESCE(retry_requested_by,created_by),?,company_id))`, [batchId, companyId, expectedVersion, userId]); } };
}
