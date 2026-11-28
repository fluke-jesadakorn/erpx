import { headers } from "next/headers";
import { nativeWorkflowReadAll } from "./native-workflow-read";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { staffSettings } from "@erpx/operations-server/public/staff-settings";
import { getTask } from "@erpx/operations-server/public/staff-task-read";
import { getLeave } from "@erpx/people-payroll-server/public/leave-read";
import { getExpense } from "@erpx/people-payroll-server/public/expense-read";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { approvalView } from "@erpx/workflow-server/public/approval-planning";
import {
  createNativeWorkflowEvidenceClient,
  nativeWorkflowEvidenceAvailable,
  nativeWorkflowEvidenceConfiguration,
  nativeWorkflowEvidenceFailure,
  type NativeWorkflowEvidenceActor,
} from "@erpx/workflow-server/public/native-workflow-evidence";
import {
  createApprovalAdministrationApplication,
  type ApprovalAdministrationRecord,
} from "@erpx/workflow-server/application/approval-administration";

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
    /** Owner-native queue read: the statement is planned by the mounted Workflow
     * wave3 command planner (WF_NATIVE_APPROVAL_ADMINISTRATION_QUEUE) and executed
     * on the caller's open company unit. This fallback carries no SQL. */
    queue: nativeQueue
      ? async () =>
          nativeQueue!.approvals.map((row) => ({
            record_type: row.record_type,
            record_id: row.record_id,
          }))
      : async (companyId, recordKind, recordId) =>
          nativeWorkflowReadAll<ApprovalAdministrationRecord>(
            "wf_native_approval_administration_queue",
            [companyId, recordId, recordKind, recordId],
            companyId,
            write,
          ),
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
