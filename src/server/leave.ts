import type { SqlDatabase } from "./runtime/types";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { sha256 } from "./crypto";
import { ApiError } from "./http";
import { assertion, executeCommand, replayCommand } from "./commands";
import {
  activeStaff,
  staffGuard,
  requestRoute,
  type RecordScope,
} from "./staff-policy";
import {
  approvalView,
  openApproval,
  prepareDecision,
  closeApprovals,
  participantSql,
  type ApprovalView,
} from "./approvals";

export const leaveDraftSchema = z.object({
  startDate: z.iso.date(),
  days: z.number().int().min(1).max(365),
  handoverNote: z.string().trim().max(2048),
  expectedVersion: z.number().int().positive(),
  saveOnly: z.boolean().default(false),
});
export const leaveDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  note: z.string().trim().max(2048).default(""),
});
export const newLeaveSchema = leaveDraftSchema.omit({
  expectedVersion: true,
  saveOnly: true,
});
export const leaveLifecycleSchema = leaveDecisionSchema.extend({
  action: z.enum(["reject", "cancel"]),
});
export type LeaveRequest = RecordScope & {
  id: string;
  number: string;
  employee_id: string;
  person_id: string | null;
  engagement_id: string | null;
  start_date: string;
  days: number;
  handover_note: string;
  state: string;
  version: number;
  created_by: string;
  prepared_by: string | null;
  decided_by: string | null;
  decision_note: string | null;
  updated_at: number;
  approval: ApprovalView;
  permittedActions: string[];
};

export async function getLeave(
  holdingId: string,
  id: string,
  context: WorkContext,
): Promise<LeaveRequest> {
  const { write } = await holdingResources(holdingId);
  // Initialize legacy routing before checking named participant access.
  const legacy = await write
    .prepare(
      "SELECT id FROM leave_requests WHERE id=? AND company_id=? AND state='pending_approval'",
    )
    .bind(id, context.companyId)
    .first();
  if (legacy) await approvalView(write, "leave", id, context);
  const row = await write
    .prepare(
      `SELECT l.* FROM leave_requests l WHERE l.id=? AND l.company_id=? AND (l.created_by=? OR l.decided_by=? OR ${participantSql("leave", "l")})`,
    )
    .bind(id, context.companyId, context.userId, context.userId, context.userId)
    .first<Omit<LeaveRequest, "approval" | "permittedActions">>();
  if (!row) throw new ApiError(404, "NOT_FOUND", "Leave request not found.");
  const approval = await approvalView(write, "leave", id, context);
  if (["draft", "rejected"].includes(row.state))
    approval.blockedReason = (
      await requestRoute(write, context.companyId, row.created_by, "leave")
    ).blockedReason;
  const own = row.created_by === context.userId;
  return {
    ...row,
    branch_id: row.branch_id || null,
    approval,
    permittedActions: [
      ...(own && ["draft", "rejected"].includes(row.state)
        ? ["save", ...(!approval.blockedReason ? ["submit"] : [])]
        : []),
      ...(own && ["draft", "rejected", "pending_approval"].includes(row.state)
        ? ["cancel"]
        : []),
      ...(approval.canDecide && row.state === "pending_approval"
        ? ["approve", "reject"]
        : []),
    ],
  };
}
export async function createLeave(
  context: WorkContext,
  input: z.infer<typeof newLeaveSchema>,
  key: string,
) {
  const { write } = await holdingResources(context.holdingId),
    id = `leave-${key}`;
  const previous = await replayCommand(
    write,
    context,
    key,
    "leave.create",
    id,
    input,
  );
  if (previous) return previous;
  const staff = await activeStaff(write, context.companyId, context.userId),
    now = Date.now();
  const employee = await write
    .prepare("SELECT id FROM employees WHERE person_id=? AND company_id=?")
    .bind(staff.person_id, context.companyId)
    .first<{ id: string }>();
  const employeeId = employee?.id ?? `employee-${staff.person_id}`;
  return executeCommand(
    write,
    context,
    key,
    "leave.create",
    input,
    id,
    1,
    { id, state: "draft", version: 1 },
    [
      staffGuard(write, key, context.companyId, context.userId),
      ...(!employee
        ? [
            write
              .prepare(
                "INSERT INTO employees(id,person_id,company_id,branch_id,department_id,display_name,status,created_at) VALUES(?,?,?,?,?,?,'active',?) ON CONFLICT DO NOTHING",
              )
              .bind(
                employeeId,
                staff.person_id,
                context.companyId,
                staff.branch_id ?? "",
                staff.department_id,
                staff.legal_name,
                now,
              ),
          ]
        : []),
      write
        .prepare(
          "INSERT INTO leave_requests(id,number,employee_id,person_id,engagement_id,company_id,branch_id,department_id,start_date,days,handover_note,created_by,created_at,updated_at) VALUES(?,?,(SELECT id FROM employees WHERE person_id=? AND company_id=?),?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          `LR-${key}`,
          staff.person_id,
          context.companyId,
          staff.person_id,
          staff.engagement_id,
          context.companyId,
          staff.branch_id ?? "",
          staff.department_id,
          input.startDate,
          input.days,
          input.handoverNote,
          context.userId,
          now,
          now,
        ),
    ],
    eventScope(staff),
  );
}
const eventScope = (row: RecordScope) => ({
  companyId: row.company_id,
  branchId: row.branch_id,
  departmentId: row.department_id,
});
function guard(
  db: SqlDatabase,
  key: string,
  id: string,
  version: number,
  states: string[],
) {
  return assertion(
    db,
    key,
    "leave-state",
    `EXISTS(SELECT 1 FROM leave_requests WHERE id=? AND version=? AND state IN (${states.map(() => "?").join(",")}))`,
    [id, version, ...states],
  );
}
export async function submitLeave(
  holdingId: string,
  id: string,
  input: z.infer<typeof leaveDraftSchema>,
  key: string,
  context: WorkContext,
) {
  const { write } = await holdingResources(holdingId),
    operation = input.saveOnly ? "leave.save" : "leave.submit";
  const prior = await replayCommand(write, context, key, operation, id, input);
  if (prior) return prior;
  const row = await getLeave(holdingId, id, context);
  if (row.created_by !== context.userId)
    throw new ApiError(
      403,
      "REQUESTER_REQUIRED",
      "Only the requester can submit or correct this request.",
    );
  const staff = await activeStaff(write, context.companyId, context.userId),
    route = await requestRoute(
      write,
      context.companyId,
      context.userId,
      "leave",
    );
  const approval = input.saveOnly
    ? null
    : await openApproval(
        write,
        key,
        "leave",
        id,
        "request",
        context.userId,
        null,
        route,
        input.expectedVersion + 1,
      );
  const state = input.saveOnly ? "draft" : "pending_approval",
    now = Date.now();
  return executeCommand(
    write,
    context,
    key,
    operation,
    input,
    id,
    input.expectedVersion + 1,
    { id, state, version: input.expectedVersion + 1 },
    [
      guard(write, key, id, input.expectedVersion, ["draft", "rejected"]),
      staffGuard(write, key, context.companyId, context.userId),
      assertion(
        write,
        key,
        "placement",
        "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=?)",
        [staff.engagement_id, staff.engagement_version],
      ),
      ...(approval?.statements ?? []),
      write
        .prepare(
          "UPDATE leave_requests SET start_date=?,days=?,handover_note=?,state=?,person_id=?,engagement_id=?,branch_id=?,department_id=?,prepared_by=?,decided_by=NULL,decision_note=NULL,version=version+1,last_command_id=?,updated_at=? WHERE id=?",
        )
        .bind(
          input.startDate,
          input.days,
          input.handoverNote,
          state,
          staff.person_id,
          staff.engagement_id,
          staff.branch_id ?? "",
          staff.department_id,
          context.userId,
          key,
          now,
          id,
        ),
    ],
    eventScope(staff),
  );
}
async function decide(
  context: WorkContext,
  id: string,
  input: z.infer<typeof leaveDecisionSchema>,
  key: string,
  approve: boolean,
) {
  const { write } = await holdingResources(context.holdingId),
    operation = approve ? "leave.approve" : "leave.reject";
  const prior = await replayCommand(write, context, key, operation, id, input);
  if (prior) return prior;
  const row = await getLeave(context.holdingId, id, context),
    decision = await prepareDecision(
      write,
      context,
      key,
      "leave",
      id,
      approve,
      input.note,
      input.expectedVersion,
    );
  const state = !approve
      ? "rejected"
      : decision.final
        ? "approved"
        : "pending_approval",
    now = Date.now();
  return executeCommand(
    write,
    context,
    key,
    operation,
    input,
    id,
    input.expectedVersion + 1,
    { id, state, version: input.expectedVersion + 1 },
    [
      guard(write, key, id, input.expectedVersion, ["pending_approval"]),
      ...decision.statements,
      write
        .prepare(
          "UPDATE leave_requests SET state=?,decision_note=?,decided_by=?,version=version+1,last_command_id=?,updated_at=? WHERE id=?",
        )
        .bind(state, input.note, context.userId, key, now, id),
      write
        .prepare("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)")
        .bind(
          crypto.randomUUID(),
          key,
          id,
          operation,
          context.userId,
          context.assignmentId,
          input.expectedVersion,
          await sha256(JSON.stringify(input)),
          now,
        ),
    ],
    eventScope(row),
  );
}
export async function approveLeave(
  _holdingId: string,
  id: string,
  input: z.infer<typeof leaveDecisionSchema>,
  key: string,
  context: WorkContext,
) {
  return decide(context, id, input, key, true);
}
export async function transitionLeave(
  context: WorkContext,
  id: string,
  input: z.infer<typeof leaveLifecycleSchema>,
  key: string,
) {
  if (input.action === "reject") return decide(context, id, input, key, false);
  const { write } = await holdingResources(context.holdingId);
  const prior = await replayCommand(
    write,
    context,
    key,
    "leave.cancel",
    id,
    input,
  );
  if (prior) return prior;
  const row = await getLeave(context.holdingId, id, context);
  if (row.created_by !== context.userId)
    throw new ApiError(
      403,
      "REQUESTER_REQUIRED",
      "Only the requester can cancel this request.",
    );
  return executeCommand(
    write,
    context,
    key,
    "leave.cancel",
    input,
    id,
    input.expectedVersion + 1,
    { id, state: "cancelled", version: input.expectedVersion + 1 },
    [
      guard(write, key, id, input.expectedVersion, [
        "draft",
        "rejected",
        "pending_approval",
      ]),
      ...closeApprovals(
        write,
        key,
        "leave",
        id,
        context.userId,
        "cancel",
        input.note,
        input.expectedVersion,
      ),
      write
        .prepare(
          "UPDATE leave_requests SET state='cancelled',decision_note=?,version=version+1,last_command_id=?,updated_at=? WHERE id=?",
        )
        .bind(input.note, key, Date.now(), id),
    ],
    eventScope(row),
  );
}
