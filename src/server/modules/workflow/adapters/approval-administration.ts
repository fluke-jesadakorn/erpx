import { holdingResources } from "@/server/modules/runtime/public/runtime";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { staffSettings } from "@/server/modules/operations/public/staff-settings";
import { getTask } from "@/server/modules/operations/public/staff-tasks";
import { getLeave } from "@/server/modules/people_payroll/public/leave";
import { getExpense } from "@/server/modules/people_payroll/public/expenses";
import { ApiError } from "@/server/modules/runtime/public/http";
import { approvalView } from "./approvals";
import { createApprovalAdministrationApplication, type ApprovalAdministrationRecord } from "../application/approval-administration";

export async function approvalAdministrationData(actor: WorkContext, kind?: string, record?: string, provided?: SqlDatabase) {
  const write = provided ?? (await holdingResources(actor.holdingId)).write;
  return createApprovalAdministrationApplication({
    settings: context => staffSettings(write, context),
    queue: async (companyId, recordKind, recordId) => (await write.prepare(
      "SELECT record_type,record_id FROM approval_instances WHERE company_id=? AND state IN ('pending','blocked') AND (?::text IS NULL OR (record_type=? AND record_id=?)) ORDER BY created_at LIMIT 50",
    ).bind(companyId, recordId, recordKind, recordId).all<ApprovalAdministrationRecord>()).results,
    record: async (context, recordKind, id) => {
      const current = recordKind === "leave"
        ? await getLeave(context.holdingId, id, context, write)
        : recordKind === "expense"
          ? await getExpense(context, id, write)
          : await getTask(write, context, id);
      return { version: current.version };
    },
    approval: (context, recordKind, id) => approvalView(write, recordKind, id, context),
    inaccessible: error => error instanceof ApiError && [403, 404].includes(error.status),
  }).load(actor, kind, record);
}
