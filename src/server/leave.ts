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
  leave_type_id?: string | null;
  end_date?: string | null;
  calculated_units?: number | null;
  cancellation_state?: string | null;
  approval: ApprovalView;
  cancellationApproval?: ApprovalView;
  start_segment?: "full"|"first_half"|"second_half";
  end_segment?: "full"|"first_half"|"second_half";
  calculation_snapshot?: string;
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
      `SELECT l.* FROM leave_requests l WHERE l.id=? AND l.company_id=? AND (l.created_by=? OR l.decided_by=? OR ${participantSql("leave", "l", "?", "?")} OR ${participantSql("leave_cancellation", "l", "?", "?")})`,
    )
    .bind(id, context.companyId, context.userId, context.userId, context.userId, context.assignmentId, context.userId, context.assignmentId)
    .first<Omit<LeaveRequest, "approval" | "permittedActions">>();
  if (!row) throw new ApiError(404, "NOT_FOUND", "Leave request not found.");
  const approval = await approvalView(write, "leave", id, context);
  if (["draft", "rejected"].includes(row.state))
    approval.blockedReason = (
      await requestRoute(write, context.companyId, row.created_by, "leave")
    ).blockedReason;
  const cancellationApproval = row.cancellation_state === "pending_review" ? await approvalView(write,"leave_cancellation",id,context) : undefined;
  const own = row.created_by === context.userId;
  return {
    ...row,
    branch_id: row.branch_id || null,
    approval,
    cancellationApproval,
    permittedActions: [
      ...(cancellationApproval?.canDecide?["approve_cancellation","reject_cancellation"]:[]),
      ...(own && ["draft", "rejected"].includes(row.state)
        ? ["save", ...(!approval.blockedReason ? ["submit"] : [])]
        : []),
      ...(own && ["draft", "rejected", "pending_approval", "approved"].includes(row.state) && row.cancellation_state !== "pending_review"
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
      ...((decision.final || !approve) ? balanceTransition(write,row,key,context.userId,approve && decision.final ? "use" : "release") : []),
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
    "leave.cancellation",
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
  if (row.state === "approved") {
    const route = await requestRoute(write, context.companyId, context.userId, "leave"), now = Date.now();
    if(row.cancellation_state === "pending_review") throw new ApiError(409,"CANCELLATION_PENDING","Cancellation is already in review.");
    if(!input.note.trim()) throw new ApiError(422,"REASON_REQUIRED","Explain why the approved leave should be cancelled.");
    if (route.blockedReason) throw new ApiError(409,"CANCELLATION_ROUTE_BLOCKED",route.blockedReason);
    const approval = await openApproval(write,key,"leave_cancellation",id,"request",context.userId,null,route,input.expectedVersion+1);
    return executeCommand(write,context,key,"leave.cancellation",input,id,input.expectedVersion+1,{id,state:"approved",cancellationState:"pending_review",version:input.expectedVersion+1},[
      guard(write,key,id,input.expectedVersion,["approved"]),
      ...approval.statements,
      write.prepare("INSERT INTO leave_cancellation_reviews(id,request_id,company_id,reason,requested_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?)").bind(`leave-cancel-${key}`,id,row.company_id,input.note,context.userId,now,now),
      write.prepare("UPDATE leave_requests SET cancellation_state='pending_review',version=version+1,last_command_id=?,updated_at=? WHERE id=?").bind(key,now,id),
    ],eventScope(row));
  }
  return executeCommand(
    write,
    context,
    key,
    "leave.cancellation",
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
      ...(row.state === "pending_approval" ? balanceTransition(write,row,key,context.userId,"release") : []),
    ],
    eventScope(row),
  );
}

export async function decideLeaveCancellation(context: WorkContext,id:string,input:z.infer<typeof leaveDecisionSchema>,key:string,approve:boolean) {
  const {write}=await holdingResources(context.holdingId);
  const operation=approve?"leave.cancellation.approve":"leave.cancellation.reject";
  const prior=await replayCommand(write,context,key,operation,id,input);if(prior)return prior;
  const row=await write.prepare("SELECT * FROM leave_requests WHERE id=? AND company_id=? AND state='approved' AND cancellation_state='pending_review'").bind(id,context.companyId).first<LeaveRequest>();
  if(!row) throw new ApiError(404,"NOT_FOUND","Pending leave cancellation review not found.");
  const decision=await prepareDecision(write,context,key,"leave_cancellation",id,approve,input.note,input.expectedVersion);
  const final=approve&&decision.final, now=Date.now();
  return executeCommand(write,context,key,approve?"leave.cancellation.approve":"leave.cancellation.reject",input,id,input.expectedVersion+1,{id,state:final?"cancelled":"approved",cancellationState:final?"approved":approve?"pending_review":"rejected",version:input.expectedVersion+1},[
    guard(write,key,id,input.expectedVersion,["approved"]),...decision.statements,
    write.prepare("UPDATE leave_requests SET state=?,cancellation_state=?,version=version+1,last_command_id=?,updated_at=? WHERE id=?").bind(final?"cancelled":"approved",final?"approved":approve?"pending_review":"rejected",key,now,id),
    write.prepare("UPDATE leave_cancellation_reviews SET state=?,decided_by=?,decision_note=?,version=version+1,updated_at=? WHERE request_id=? AND state='pending_approval'").bind(final?"approved":approve?"pending_approval":"rejected",context.userId,input.note,now,id),
    ...(final?balanceTransition(write,row,key,context.userId,"reverse"):[]),
  ],eventScope(row));
}

/** Release only reservations actually recorded; rejection never creates usage. */
function balanceTransition(db:SqlDatabase,row:LeaveRequest,key:string,actor:string,action:"use"|"release"|"reverse") {
 const source=action==='reverse'?"'usage','usage_reversal'":"'reservation','reservation_release'";
 const insert=(kind:string,multiplier:number)=>db.prepare(`INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,request_id,kind,units,effective_date,entitlement_year,command_key,note,created_by,created_at)
 SELECT ?||':'||entitlement_year,company_id,engagement_id,leave_type_id,request_id,?,sum(units)*?,entitlement_year||'-01-01',entitlement_year,?||':'||entitlement_year,?,?,?
 FROM leave_balance_entries WHERE request_id=? AND company_id=? AND kind IN (${source}) GROUP BY company_id,engagement_id,leave_type_id,request_id,entitlement_year HAVING sum(units)<0`)
 .bind(`${key}:${kind}`,kind,multiplier,key,action==='reverse'?'Approved cancellation':action==='use'?'Approved leave':'Released reservation',actor,Date.now(),row.id,row.company_id);
 // Usage must read the reservation before its release is inserted.
 return action==='use'?[insert('usage',1),insert('reservation_release',-1)]:[insert(action==='reverse'?'usage_reversal':'reservation_release',-1)];
}
