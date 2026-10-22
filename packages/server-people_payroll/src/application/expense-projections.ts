import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { StaffActor, RecordScope } from "@erpx/server-operations/public/staff-policy";
import { expensePermittedActions } from "@erpx/server-people_payroll/domain/expense-permitted-actions";
import type { ExpenseRecord } from "@erpx/server-people_payroll/application/expense-commands";
import type { ExpenseCommandPort } from "@erpx/server-people_payroll/application/expense-commands";

export function createExpensePaymentReader(ports: { error: ExpenseCommandPort["error"]; draft(actor: WorkContext, id: string): Promise<ExpenseRecord | null>; expense(actor: WorkContext, id: string): Promise<ExpenseRecord>; payers(row: ExpenseRecord): Promise<string[]> }) {
  return async (actor: WorkContext, id: string, editable = false) => {
    const draft = await ports.draft(actor, id);
    if (!draft) throw ports.error({ status: 404, code: "PAYMENT_DRAFT_NOT_FOUND", message: "Payment evidence not found in this company." });
    const expense = await ports.expense(actor, String(draft.expense_id));
    if (editable && (draft.status !== "draft" || expense.workflow_stage !== "payment" || !(await ports.payers(expense)).includes(actor.userId)))
      throw ports.error({ status: 403, code: "TREASURY_REQUIRED", message: "An independent Treasury operator must prepare this payment." });
    return { draft, expense };
  };
}

export type ExpenseApprovalFacts = { canDecide: boolean; blockedReason: string | null; instance: { state: string } | null; currentApprover: { approver_user_id: string; approver_name?: string | null } | null; eligibleApproverIds?: string[]; steps: { route_snapshot_json?: string | null }[] };
export type ExpenseFlowFacts = { canAct: boolean; current?: { primary?: { name: string | null } | null } | null; blocker?: string | null };
export interface ExpensePayerPorts {
  approvedPeople(row: ExpenseRecord): Promise<string[]>;
  roleUsers(scope: RecordScope, role: string): Promise<string[]>;
}
export function createExpensePayers(ports: ExpensePayerPorts) {
  return async (row: ExpenseRecord) => {
    const history = await ports.approvedPeople(row), accounting = JSON.parse(String(row.accounting_json ?? "{}")) as { preparedBy?: string };
    const forbidden = new Set([row.claimant_user_id, row.created_by, row.prepared_by, accounting.preparedBy, ...history]);
    return (await ports.roleUsers(scope(row), "treasury")).filter(id => !forbidden.has(id));
  };
}
export interface ExpenseWorkflowProjectionPorts {
  approval(actor: WorkContext, id: string): Promise<ExpenseApprovalFacts>;
  payers(row: ExpenseRecord): Promise<string[]>;
  flow(actor: WorkContext, id: string): Promise<ExpenseFlowFacts>;
}
export function createExpenseWorkflowFields(ports: ExpenseWorkflowProjectionPorts) {
  return async (actor: WorkContext, row: ExpenseRecord): Promise<ExpenseRecord> => {
    if (row.cancelled_at) return { ...row, status: "cancelled", workflow_stage: "cancelled", can_decide: 0 };
    const view = await ports.approval(actor, String(row.id)), candidates = view.eligibleApproverIds ?? [], payers = row.workflow_stage === "payment" ? await ports.payers(row) : [], flow = await ports.flow(actor, String(row.id));
    return {
      ...row, spending_template_version: JSON.parse(view.steps[0]?.route_snapshot_json ?? "{}").templateVersion ?? null,
      can_post: row.workflow_stage === "gl_posting" && flow.canAct ? 1 : 0, can_decide: view.canDecide && flow.canAct ? 1 : 0,
      current_approver_id: view.canDecide ? actor.userId : view.currentApprover?.approver_user_id ?? null,
      current_approver_name: flow.current?.primary?.name ?? null, approval_stage: view.instance?.state ?? null,
      approval_blocked_reason: flow.blocker ?? view.blockedReason ?? (row.workflow_stage === "payment" && !payers.length ? "An independent Treasury operator is required to record payment." : null),
      eligible_approvers_json: JSON.stringify(candidates), can_pay: payers.includes(actor.userId) && flow.canAct ? 1 : 0,
    };
  };
}

export function createCurrentExpenseReader(ports: { auditReader(actor: StaffActor, row: ExpenseRecord): Promise<boolean>; participant(actor: StaffActor, row: ExpenseRecord): Promise<boolean>; canDecide(actor: StaffActor, id: string): Promise<boolean> }) {
  return async (actor: StaffActor, row: ExpenseRecord) => {
    if (row.workflow_version !== 2 || [row.claimant_user_id, row.created_by, row.approved_by].includes(actor.userId)) return true;
    if (await ports.auditReader(actor, row)) return true;
    if (await ports.participant(actor, row)) return true;
    return ports.canDecide(actor, String(row.id));
  };
}
export function createExpenseApprovalFields(ports: { requirements(row: ExpenseRecord): Promise<string[]>; current(actor: WorkContext, row: ExpenseRecord): Promise<ExpenseRecord>; approval(actor: WorkContext, id: string): Promise<ExpenseApprovalFacts>; department(actor: WorkContext, row: ExpenseRecord): Promise<{ blockedReason: string | null }> }) {
  return async (actor: WorkContext, row: ExpenseRecord): Promise<ExpenseRecord> => {
    if (row.workflow_version === 2 && ["draft", "rejected"].includes(String(row.status)) && !row.cancelled_at) {
      const missing = await ports.requirements(row);
      return { ...row, setup_requirements_json: JSON.stringify(missing), approval_blocked_reason: missing[0] ?? null, can_decide: 0 };
    }
    if (row.workflow_version === 2) return ports.current(actor, row);
    const view = await ports.approval(actor, String(row.id)), blocked = ["draft", "rejected"].includes(String(row.status)) && !row.cancelled_at ? (await ports.department(actor, row)).blockedReason : view.blockedReason;
    return { ...row, status: row.cancelled_at ? "cancelled" : row.status, current_approver_id: view.currentApprover?.approver_user_id ?? null, current_approver_name: view.currentApprover?.approver_name ?? null, approval_stage: view.instance?.state ?? null, approval_blocked_reason: blocked, can_decide: view.canDecide ? 1 : 0 };
  };
}

export interface ExpenseDetailPorts<Approval> {
  expense(actor: WorkContext, id: string): Promise<ExpenseRecord>;
  attachments(actor: WorkContext, id: string): Promise<ExpenseRecord[]>;
  history(actor: WorkContext, id: string, expense: ExpenseRecord): Promise<ExpenseRecord[]>;
  payments(actor: WorkContext, expense: ExpenseRecord): Promise<ExpenseRecord[]>;
  decisions(actor: WorkContext, id: string): Promise<ExpenseRecord[]>;
  paymentDrafts(actor: WorkContext, id: string): Promise<ExpenseRecord[]>;
  paymentAttachments(actor: WorkContext, id: string): Promise<ExpenseRecord[]>;
  approval(actor: WorkContext, id: string): Promise<Approval>;
}
export function createExpenseDetail<Approval>(ports: ExpenseDetailPorts<Approval>) {
  return async (actor: WorkContext, id: string) => {
    const expense = await ports.expense(actor, id);
    const [attachments, history, payments, decisions] = await Promise.all([ports.attachments(actor, id), ports.history(actor, id, expense), ports.payments(actor, expense), ports.decisions(actor, id)]);
    const [paymentDrafts, paymentAttachments] = await Promise.all([ports.paymentDrafts(actor, id), ports.paymentAttachments(actor, id)]);
    return { paymentDrafts, paymentAttachments, approval: await ports.approval(actor, id), permittedActions: expensePermittedActions(actor, expense), expense, attachments, history, payments, decisions };
  };
}
export function createExpenseOptions(ports: { people(actor: WorkContext): Promise<ExpenseRecord[]>; accounts(actor: WorkContext): Promise<ExpenseRecord[]>; periods(actor: WorkContext): Promise<ExpenseRecord[]>; payees(actor: WorkContext): Promise<ExpenseRecord[]> }) {
  return async (actor: WorkContext) => {
    const [people, accounts, periods, payees] = await Promise.all([ports.people(actor), ports.accounts(actor), ports.periods(actor), ports.payees(actor)]);
    return { people, accounts, periods, payees, selfUserId: actor.userId, companyId: actor.companyId, assignmentId: actor.assignmentId };
  };
}
function scope(row: ExpenseRecord): RecordScope { return { company_id: String(row.company_id), branch_id: row.branch_id ? String(row.branch_id) : null, department_id: row.department_id ? String(row.department_id) : null }; }
