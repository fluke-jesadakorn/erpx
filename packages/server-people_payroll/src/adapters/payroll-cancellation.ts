import { withPayrollPurchasedCommand } from "@erpx/server-people_payroll/adapters/payroll-purchases";
import {peoplePayrollFinalOwnerProjections as workflowFinalOwnerProjections} from "@erpx/server-workflow/public/people-payroll-final-owner-projections";
const workflowFinalSources=workflowFinalOwnerProjections();
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { resolveFlow } from "@erpx/server-workflow/public/flow-routing";
import { ApiError } from "@erpx/server-runtime/public/http";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { BusinessRuleError } from '@erpx/server-shared/index';
import { payrollCancellationGrant, payrollCancellationGrantLock, payrollCancellationGrantGuard } from '@erpx/server-identity_access/public/payroll-cancellation-grant';
import { createPayrollCancellationApplication, type CancellationMutation } from '@erpx/server-people_payroll/application/payroll-cancellation';
import type { CancellationRun } from '@erpx/server-people_payroll/domain/payroll-cancellation';
export { payrollCancelSchema } from '@erpx/server-people_payroll/domain/payroll-cancellation';
export async function cancelPayrollRun(actor: WorkContext, runId: string, input: import("@erpx/server-people_payroll/domain/payroll-cancellation").PayrollCancelInput, key: string) {
    return withPayrollPurchasedCommand(actor, {key, operation: "people.payroll.cancel", purpose: "settle-existing"}, tx => cancelPayrollRunIn(actor, runId, input, key, tx));
}
async function cancelPayrollRunIn(actor: WorkContext, runId: string, input: import("@erpx/server-people_payroll/domain/payroll-cancellation").PayrollCancelInput, key: string, provided?: SqlDatabase) {
    let write: SqlDatabase;
    try {
        return await createPayrollCancellationApplication({ clock: { now: Date.now }, authorize: requirePermission, prepare: async (actor) => { write = provided ?? (await holdingResources(actor.holdingId)).write; }, currentGrant: actor => payrollCancellationGrant(write, actor), replay: (...args) => replayCommand(write, ...args), run: (actor, id) => write.prepare('SELECT r.*,to_jsonb(r)::text source_snapshot FROM hr_payroll_runs r WHERE r.id=? AND r.company_id=?').bind(id, actor.companyId).first<CancellationRun>(), flow: (actor, id) => resolveFlow(write, actor, 'payroll', id), commit: async <Result>(m: CancellationMutation, _result: Result) => {
                const { actor, runId, input, key, run, evidenceId, now, step } = m;
                return executeCommand(write, actor, key, "people.payroll.cancel", input, runId, input.expectedVersion + 1, _result, [
                    payrollCancellationGrantLock(write, actor),
                    payrollCancellationGrantGuard(write, key, actor),
                    write.prepare("SELECT id FROM hr_payroll_runs WHERE id=? AND company_id=? FOR UPDATE").bind(runId, actor.companyId),
                    assertion(write, key, "payroll-cancellation-source", `EXISTS(SELECT 1 FROM hr_payroll_runs r WHERE r.id=? AND r.company_id=?
        AND r.version=? AND to_jsonb(r)=?::jsonb AND r.status IN ('draft','calculating','failed','preview') AND r.mode!='final_pay'
        AND r.submitted_by IS NULL AND r.approved_by IS NULL AND r.posted_by IS NULL)`, [runId, actor.companyId, input.expectedVersion, run.source_snapshot]),
                    assertion(write, key, "payroll-cancellation-owner", `EXISTS(SELECT 1 FROM hr_payroll_runs r WHERE r.id=? AND r.company_id=?
        AND (iam_same_person(r.created_by,?,r.company_id) OR EXISTS(SELECT 1 ${workflowFinalSources.payroll_cancellation_flow_routes_f_from} WHERE f.company_id=r.company_id
          AND f.kind='payroll' AND f.record_id=r.id AND iam_same_person(f.steps_json::jsonb -> ? ->> 'primaryUserId',?,r.company_id))))`, [runId, actor.companyId, actor.userId, step, actor.userId]),
                    write.prepare("UPDATE hr_payroll_runs SET status='cancelled',version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?")
                        .bind(now, runId, actor.companyId, input.expectedVersion),
                    write.prepare(`INSERT INTO hr_payroll_run_cancellations(id,company_id,run_id,command_id,cancelled_by,reason,prior_status,prior_version,run_revision,source_snapshot_json,created_at)
        VALUES(?,?,?,?,?,?,?,?,?,?,?)`).bind(evidenceId, actor.companyId, runId, key, actor.userId, input.reason, run.status, input.expectedVersion, run.revision, run.source_snapshot, now),
                ]);
            } }).cancelPayrollRun(actor, runId, input, key);
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
