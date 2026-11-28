import {workflowFinalOwnerProjections as peoplepayrollFinalOwnerProjections} from "@erpx/people-payroll-server/public/workflow-final-owner-projections";
import {nativeWorkflowWrite} from './native-workflow-write';
import {receiptRecordIdSql} from './receipt-sql';
const peoplepayrollFinalSources=peoplepayrollFinalOwnerProjections();
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
export function payrollWorkflowFacts(database: SqlDatabase) {
    return { readCommandCurrent(id: string) {
            return database.prepare(receiptRecordIdSql).bind(id).first<{
                record_id: string;
            }>();
        },
        stageTransitionCompensationEffect2(value: string, key: string, compensationId: string, value2: string, userId: string, assignmentId: string, expectedVersion: number, value3: string, now: number) { return nativeWorkflowWrite('wf_native_payroll_owner_facts_11', [value, key, compensationId, value2, userId, assignmentId, expectedVersion, value3, now]); },
        stageRejectTimeEntryEffect2(value: string, key: string, entryId: string, value2: "people.time.reject", userId: string, assignmentId: string, expectedVersion: number, value3: string, now: number) { return nativeWorkflowWrite('wf_native_payroll_owner_facts_12', [value, key, entryId, value2, userId, assignmentId, expectedVersion, value3, now]); },
        stageApproveTimeEntryEffect2(value: string, key: string, entryId: string, value2: "people.time.approve", userId: string, assignmentId: string, expectedVersion: number, key2: string, now: number) { return nativeWorkflowWrite('wf_native_payroll_owner_facts_13', [value, key, entryId, value2, userId, assignmentId, expectedVersion, key2, now]); },
        stageTransitionCountryPackEffect2(value: string, commandId: string, packId: string, value2: string, userId: string, assignmentId: string, record_version: number, sourceDigest: string, now: number) { return nativeWorkflowWrite('wf_native_payroll_owner_facts_14', [value, commandId, packId, value2, userId, assignmentId, record_version, sourceDigest, now]); },
        stageApprovePayrollRunEffect2(value: string, key: string, runId: string, value2: "people.payroll.approve", userId: string, assignmentId: string, expectedVersion: number, now: number, runId2: string) { return nativeWorkflowWrite('wf_native_payroll_owner_facts_15', [value, key, runId, value2, userId, assignmentId, expectedVersion, now, runId2]); },
        stageApprovePaymentBatchEffect2(value: string, commandId: string, batchId: string, value2: "people.payment-batch.approve", userId: string, assignmentId: string, expectedVersion: number, commandId2: string, now: number) { return nativeWorkflowWrite('wf_native_payroll_owner_facts_16', [value, commandId, batchId, value2, userId, assignmentId, expectedVersion, commandId2, now]); } };
}
