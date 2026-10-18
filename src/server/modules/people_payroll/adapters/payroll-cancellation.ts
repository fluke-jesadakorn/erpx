import { withPayrollPurchasedCommand } from "./payroll-purchases";
import {peoplePayrollFinalOwnerProjections as workflowFinalOwnerProjections} from "../../workflow/public/people-payroll-final-owner-projections";
const workflowFinalSources=workflowFinalOwnerProjections();
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { resolveFlow } from "@/server/modules/workflow/public/flow-routing";
import { ApiError } from "@/server/modules/runtime/public/http";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { BusinessRuleError } from '../../shared';
import { payrollCancellationGrant, payrollCancellationGrantLock, payrollCancellationGrantGuard } from '../../identity_access/public/payroll-cancellation-grant';
import { createPayrollCancellationApplication, type CancellationMutation } from '../application/payroll-cancellation';
import type { CancellationRun } from '../domain/payroll-cancellation';
export { payrollCancelSchema } from '../domain/payroll-cancellation';
export async function cancelPayrollRun(actor: WorkContext, runId: string, input: import('../domain/payroll-cancellation').PayrollCancelInput, key: string) {
    return withPayrollPurchasedCommand(actor, {key, operation: "people.payroll.cancel", purpose: "settle-existing"}, tx => cancelPayrollRunIn(actor, runId, input, key, tx));
}
async function cancelPayrollRunIn(actor: WorkContext, runId: string, input: import('../domain/payroll-cancellation').PayrollCancelInput, key: string, provided?: SqlDatabase) {
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
