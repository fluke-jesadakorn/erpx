import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { staffSettings } from "@erpx/server-operations/public/staff-settings";
import {getTask} from '@erpx/server-operations/public/staff-task-read';
import {getLeave} from '@erpx/server-people_payroll/public/leave-read';
import {getExpense} from '@erpx/server-people_payroll/public/expense-read';
import { ApiError } from "@erpx/server-runtime/public/http";
import {approvalView} from '@erpx/server-workflow/public/approval-planning';
import { createApprovalAdministrationApplication, type ApprovalAdministrationRecord } from "@erpx/server-workflow/application/approval-administration";

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
