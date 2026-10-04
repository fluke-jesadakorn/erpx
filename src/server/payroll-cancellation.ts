import "server-only";

import { z } from "zod";
import { samePerson } from "@/lib/iam";
import type { WorkContext } from "./authorization";
import { assertion, executeCommand, replayCommand } from "./commands";
import { resolveFlow } from "./flow-routing";
import { ApiError } from "./http";
import { requirePermission } from "./iam-permissions";
import { holdingResources } from "./runtime";
import type { SqlDatabase } from "./runtime/types";

export const payrollCancelSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(8).max(1000),
}).strict();

const cancellableStates = new Set(["draft", "calculating", "failed", "preview"]);
type Run = {
  id: string; company_id: string; status: string; mode: string; version: number;
  revision: number; created_by: string; submitted_by: string | null;
  approved_by: string | null; posted_by: string | null; source_snapshot: string;
};

const currentGrant = `EXISTS(SELECT 1 FROM assignments a
  WHERE a.id=? AND a.company_id=? AND a.user_id=? AND a.role=? AND a.scope_kind='holding'
  AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
  AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
  AND iam_role_allows(a.role,a.company_id,'payroll.prepare')
  AND EXISTS(SELECT 1 FROM organization_nodes c WHERE c.id=a.company_id AND c.status='active'))`;

async function assertCurrentGrant(db: SqlDatabase, actor: WorkContext) {
  const grant = await db.prepare(`SELECT 1 WHERE ${currentGrant}`)
    .bind(actor.assignmentId, actor.companyId, actor.userId, actor.role).first();
  if (!grant) throw new ApiError(403, "PAYROLL_CANCELLATION_FORBIDDEN", "Your current Company role does not permit payroll preparation.");
}

/** Cancel only an unsubmitted source; original items and issued review bytes stay retained. */
export async function cancelPayrollRun(actor: WorkContext, runId: string, input: z.infer<typeof payrollCancelSchema>, key: string) {
  actor = requirePermission(actor, "payroll.prepare");
  if (actor.branchId || actor.departmentId) throw new ApiError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Use a Company Payroll preparation assignment.");
  const { write } = await holdingResources(actor.holdingId);
  await assertCurrentGrant(write, actor);
  const prior = await replayCommand(write, actor, key, "people.payroll.cancel", runId, input);
  if (prior) return prior;
  const run = await write.prepare("SELECT r.*,to_jsonb(r)::text source_snapshot FROM hr_payroll_runs r WHERE r.id=? AND r.company_id=?")
    .bind(runId, actor.companyId).first<Run>();
  if (!run) throw new ApiError(404, "PAYROLL_RUN_NOT_FOUND", "Payroll run not found in this Company.");
  if (Number(run.version) !== input.expectedVersion) throw new ApiError(409, "PAYROLL_RUN_CHANGED", "Reload the payroll run before cancelling it.");
  if (!cancellableStates.has(run.status) || run.mode === "final_pay" || run.submitted_by || run.approved_by || run.posted_by)
    throw new ApiError(409, "PAYROLL_CANCELLATION_UNAVAILABLE", "Only an unsubmitted ordinary payroll draft or preview can be cancelled.");
  const flow = await resolveFlow(write, actor, "payroll", runId);
  const step = run.status === "preview" ? "preview" : "calculate";
  const primary = flow.steps.find(candidate => candidate.key === step)?.primary?.userId;
  if (!samePerson(actor, run.created_by) && primary !== actor.userId)
    throw new ApiError(403, "PAYROLL_CANCELLATION_OWNER_REQUIRED", "The creator or named payroll preparer must cancel this run.");
  const evidenceId = `payroll-cancellation-${key}`, now = Date.now();
  return executeCommand(write, actor, key, "people.payroll.cancel", input, runId, input.expectedVersion + 1,
    { id: runId, status: "cancelled", version: input.expectedVersion + 1, cancellationId: evidenceId }, [
      write.prepare("SELECT id FROM assignments WHERE id=? FOR SHARE").bind(actor.assignmentId),
      assertion(write, key, "payroll-cancellation-current-grant", currentGrant, [actor.assignmentId, actor.companyId, actor.userId, actor.role]),
      write.prepare("SELECT id FROM hr_payroll_runs WHERE id=? AND company_id=? FOR UPDATE").bind(runId, actor.companyId),
      assertion(write, key, "payroll-cancellation-source", `EXISTS(SELECT 1 FROM hr_payroll_runs r WHERE r.id=? AND r.company_id=?
        AND r.version=? AND to_jsonb(r)=?::jsonb AND r.status IN ('draft','calculating','failed','preview') AND r.mode!='final_pay'
        AND r.submitted_by IS NULL AND r.approved_by IS NULL AND r.posted_by IS NULL)`, [runId, actor.companyId, input.expectedVersion, run.source_snapshot]),
      assertion(write, key, "payroll-cancellation-owner", `EXISTS(SELECT 1 FROM hr_payroll_runs r WHERE r.id=? AND r.company_id=?
        AND (iam_same_person(r.created_by,?,r.company_id) OR EXISTS(SELECT 1 FROM flow_routes f WHERE f.company_id=r.company_id
          AND f.kind='payroll' AND f.record_id=r.id AND iam_same_person(f.steps_json::jsonb -> ? ->> 'primaryUserId',?,r.company_id))))`,
        [runId, actor.companyId, actor.userId, step, actor.userId]),
      write.prepare("UPDATE hr_payroll_runs SET status='cancelled',version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?")
        .bind(now, runId, actor.companyId, input.expectedVersion),
      write.prepare(`INSERT INTO hr_payroll_run_cancellations(id,company_id,run_id,command_id,cancelled_by,reason,prior_status,prior_version,run_revision,source_snapshot_json,created_at)
        VALUES(?,?,?,?,?,?,?,?,?,?,?)`).bind(evidenceId, actor.companyId, runId, key, actor.userId, input.reason, run.status, input.expectedVersion, run.revision, run.source_snapshot, now),
    ]);
}
