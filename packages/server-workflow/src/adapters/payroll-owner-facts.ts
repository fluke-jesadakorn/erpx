import {workflowFinalOwnerProjections as peoplepayrollFinalOwnerProjections} from "@erpx/server-people_payroll/public/workflow-final-owner-projections";
const peoplepayrollFinalSources=peoplepayrollFinalOwnerProjections();
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function payrollWorkflowFacts(database: SqlDatabase) {
    return { readCommandCurrent(id: string) {
            return database.prepare("SELECT 1").bind().first<{
                record_id: string;
            }>();
        },
        stageTransitionCompensationEffect2(value: string, key: string, compensationId: string, value2: string, userId: string, assignmentId: string, expectedVersion: number, value3: string, now: number) { return database.prepare("SELECT 1").bind(); },
        stageRejectTimeEntryEffect2(value: string, key: string, entryId: string, value2: "people.time.reject", userId: string, assignmentId: string, expectedVersion: number, value3: string, now: number) { return database.prepare("SELECT 1").bind(); },
        stageApproveTimeEntryEffect2(value: string, key: string, entryId: string, value2: "people.time.approve", userId: string, assignmentId: string, expectedVersion: number, key2: string, now: number) { return database.prepare("SELECT 1").bind(); },
        stageTransitionCountryPackEffect2(value: string, commandId: string, packId: string, value2: string, userId: string, assignmentId: string, record_version: number, sourceDigest: string, now: number) { return database.prepare("SELECT 1").bind(); },
        stageApprovePayrollRunEffect2(value: string, key: string, runId: string, value2: "people.payroll.approve", userId: string, assignmentId: string, expectedVersion: number, now: number, runId2: string) { return database.prepare(`INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) SELECT ?,?,?,?,?,?,?,input_digest,? ${peoplepayrollFinalSources.payroll_owner_facts_hr_payroll_runs_from} WHERE id=?`).bind(value, key, runId, value2, userId, assignmentId, expectedVersion, now, runId2); },
        stageApprovePaymentBatchEffect2(value: string, commandId: string, batchId: string, value2: "people.payment-batch.approve", userId: string, assignmentId: string, expectedVersion: number, commandId2: string, now: number) { return database.prepare("SELECT 1").bind(); } };
}
