import { headers } from "next/headers";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { staffSettings } from "@erpx/server-operations/public/staff-settings";
import { getTask } from "@erpx/server-operations/public/staff-task-read";
import { getLeave } from "@erpx/server-people_payroll/public/leave-read";
import { getExpense } from "@erpx/server-people_payroll/public/expense-read";
import { ApiError } from "@erpx/server-runtime/public/http";
import { approvalView } from "@erpx/server-workflow/public/approval-planning";
import {
  createNativeWorkflowEvidenceClient,
  nativeWorkflowEvidenceAvailable,
  nativeWorkflowEvidenceConfiguration,
  nativeWorkflowEvidenceFailure,
  type NativeWorkflowEvidenceActor,
} from "@erpx/server-workflow/public/native-workflow-evidence";
import {
  createApprovalAdministrationApplication,
  type ApprovalAdministrationRecord,
} from "@erpx/server-workflow/application/approval-administration";

function nativeActor(actor: WorkContext): NativeWorkflowEvidenceActor {
  if (!actor.authority || !actor.iam)
    throw new ApiError(
      409,
      "WORK_CONTEXT_CHANGED",
      "Your work role changed. Reload and review before retrying.",
    );
  return {
    userId: actor.userId,
    holdingId: actor.holdingId,
    holdingSlug: actor.holdingSlug,
    companyId: actor.companyId,
    branchId: actor.branchId,
    departmentId: actor.departmentId,
    assignmentId: actor.assignmentId,
    role: actor.role,
    scopeKind: actor.assignmentScope,
    authority: {
      revision: actor.authority.revision,
      assignmentVersion: actor.authority.assignmentVersion,
    },
    permissions: actor.iam.permissions,
  };
}

function nativeFailure(error: unknown): never {
  const failure = nativeWorkflowEvidenceFailure(error);
  if (failure)
    throw new ApiError(failure.status, failure.code, failure.message);
  throw error;
}

export async function approvalAdministrationData(
  actor: WorkContext,
  kind?: string,
  record?: string,
  provided?: SqlDatabase,
) {
  const write = provided ?? (await holdingResources(actor.holdingId)).write;
  const native = nativeWorkflowEvidenceAvailable();
  let incoming: Awaited<ReturnType<typeof headers>> | undefined;
  let nativeQueue:
    | Awaited<ReturnType<ReturnType<typeof createNativeWorkflowEvidenceClient>["queue"]>>
    | undefined;
  if (native) {
    try {
      incoming = await headers();
      const raw = process.env.ERPX_NATIVE_WORKFLOW_CONFIG;
      const client = createNativeWorkflowEvidenceClient(
        nativeWorkflowEvidenceConfiguration(raw!),
      );
      if (!record) nativeQueue = await client.queue(incoming, nativeActor(actor));
    } catch (error) {
      nativeFailure(error);
    }
  }
  const loaded = await createApprovalAdministrationApplication({
    settings: (context) => staffSettings(write, context),
    queue: nativeQueue
      ? async () =>
          nativeQueue!.approvals.map((row) => ({
            record_type: row.record_type,
            record_id: row.record_id,
          }))
      : async (companyId, recordKind, recordId) =>
          (
            await write
              .prepare(
                "SELECT record_type,record_id FROM approval_instances WHERE company_id=? AND state IN ('pending','blocked') AND (?::text IS NULL OR (record_type=? AND record_id=?)) ORDER BY created_at LIMIT 50",
              )
              .bind(companyId, recordId, recordKind, recordId)
              .all<ApprovalAdministrationRecord>()
          ).results,
    record: async (context, recordKind, id) => {
      const current =
        recordKind === "leave"
          ? await getLeave(context.holdingId, id, context, write)
          : recordKind === "expense"
            ? await getExpense(context, id, write)
            : await getTask(write, context, id);
      return { version: current.version };
    },
    approval: (context, recordKind, id) =>
      approvalView(write, recordKind, id, context),
    inaccessible: (error) =>
      error instanceof ApiError && [403, 404].includes(error.status),
  }).load(actor, kind, record);
  if (!loaded) return null;
  if (!native || !record || !kind) return { ...loaded, nativeEvidence: null };
  try {
    const raw = process.env.ERPX_NATIVE_WORKFLOW_CONFIG;
    const client = createNativeWorkflowEvidenceClient(
      nativeWorkflowEvidenceConfiguration(raw!),
    );
    const nativeEvidence = await client.evidence(
      incoming!,
      nativeActor(actor),
      kind,
      record,
    );
    return { ...loaded, nativeEvidence };
  } catch (error) {
    return nativeFailure(error);
  }
}
