import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { ExpenseRecordFacts, ExpenseViolation } from "../domain/expense-policy";
import { expenseAccountingViolation, mayEditExpense } from "../domain/expense-policy";
import { samePerson } from "@/lib/iam";
import { hasPermission } from "@/lib/permissions";

export type ExpenseRecord = ExpenseRecordFacts;
export type ExpenseAccountingInput = { expectedVersion: number; categoryAccountId: string; tax: string; entryDate: string; periodId: string; note: string };
export type ExpensePostingInput = { expectedVersion: number; periodId: string; entryDate: string };
export type ExpensePaymentInput = { expectedVersion: number; date: string; amount: string; reference: string };
export type ExpenseDraftInput = { expectedVersion: number; payeePartyId?: string; requestKind?: "expense" | "payment"; personId?: string; expenseDate?: string; merchant?: string; receiptReference?: string; businessPurpose?: string; currency?: string; amount?: string; tax?: string; categoryAccountId?: string };
export type ExpenseStaff = { user_id: string; person_id: string; legal_name: string; engagement_id: string; branch_id: string | null; department_id: string | null };
export type ExpenseScope = { companyId: string; branchId: string | null; departmentId: string | null };
export type ExpenseCommandActor = Pick<WorkContext, "userId" | "assignmentId" | "role" | "companyId" | "branchId" | "departmentId" | "authority" | "iam">;

/** A native adapter owns the transaction work. Application code cannot inspect SQL. */
export interface ExpenseMutation<Result = Record<string, unknown>> { readonly result: Result; readonly native: unknown }
export interface ExpenseCommandPort<Actor extends ExpenseCommandActor = WorkContext> {
  replay(actor: Actor, key: string, operation: string, id: string, input: unknown): Promise<Record<string, unknown> | null>;
  commit(actor: Actor, key: string, operation: string, input: unknown, id: string, version: number, mutation: ExpenseMutation, scope?: ExpenseScope): Promise<Record<string, unknown>>;
  error(violation: ExpenseViolation): Error;
}
export interface ExpenseAccountingPorts extends ExpenseCommandPort {
  authority(actor: WorkContext): Promise<unknown>;
  expense(actor: WorkContext, id: string): Promise<ExpenseRecord>;
  approval(actor: WorkContext, id: string): Promise<{ canDecide: boolean; instance: { id: string } | null }>;
  money(value: string): number;
  prepareAccounting(actor: WorkContext, key: string, id: string, input: ExpenseAccountingInput, row: ExpenseRecord, approval: { instance: { id: string } | null }, authority: unknown, tax: number, net: number): Promise<ExpenseMutation>;
}

export function createSaveExpenseAccounting(ports: ExpenseAccountingPorts) {
  return async (actor: WorkContext, id: string, input: ExpenseAccountingInput, key: string) => {
    const prior = await ports.replay(actor, key, "expense.accounting.save", id, input);
    if (prior) return prior;
    const authority = await ports.authority(actor);
    const row = await ports.expense(actor, id), approval = await ports.approval(actor, id);
    if (row.workflow_version !== 2 || row.workflow_stage !== "accounts_review" || !approval.canDecide)
      throw ports.error({ status: 403, code: "ACCOUNTS_REVIEW_REQUIRED", message: "Only the current independent Accounts queue can prepare accounting details." });
    const gross = Number(row.amount_minor) + Number(row.tax_minor), tax = ports.money(input.tax), net = gross - tax;
    const violation = expenseAccountingViolation(gross, tax, String(row.expense_date), input.entryDate);
    if (violation) throw ports.error(violation);
    const mutation = await ports.prepareAccounting(actor, key, id, input, row, approval, authority, tax, net);
    return ports.commit(actor, key, "expense.accounting.save", input, id, input.expectedVersion + 1, mutation, recordScope(row));
  };
}

export interface ExpensePostingPorts extends ExpenseCommandPort {
  expense(actor: WorkContext, id: string): Promise<ExpenseRecord>;
  legacyPoster(actor: WorkContext): WorkContext;
  preparePosting(actor: WorkContext, row: ExpenseRecord, input: ExpensePostingInput, key: string): Promise<ExpenseMutation>;
}
export function createPostExpense(ports: ExpensePostingPorts) {
  return async (actor: WorkContext, id: string, input: ExpensePostingInput, key: string) => {
    const prior = await ports.replay(actor, key, "expense.post", id, input);
    if (prior) return prior;
    const row = await ports.expense(actor, id);
    if (row.workflow_version === 2) {
      if (row.workflow_stage !== "gl_posting" || row.status !== "approved" || row.can_post !== 1)
        throw ports.error({ status: 403, code: "GL_POSTER_REQUIRED", message: "Only the assigned independent accountant can post approved accounting." });
      const accounting = JSON.parse(String(row.accounting_json ?? "{}")) as { preparedBy?: string; periodId?: string; entryDate?: string };
      if (samePerson(actor, accounting.preparedBy))
        throw ports.error({ status: 403, code: "SELF_POST_FORBIDDEN", message: "The accounting preparer cannot post their own preparation." });
      if (input.periodId !== accounting.periodId || input.entryDate !== accounting.entryDate)
        throw ports.error({ status: 422, code: "APPROVED_ACCOUNTING_REQUIRED", message: "Use the approved accounting date and period." });
    } else {
      actor = ports.legacyPoster(actor);
      if (!hasPermission(actor, "expense.approve") || actor.branchId || actor.departmentId)
        throw ports.error({ status: 403, code: "FINANCE_APPROVER_REQUIRED", message: "A holding-level finance approver must post expense obligations." });
    }
    return ports.commit(actor, key, "expense.post", input, id, input.expectedVersion + 1, await ports.preparePosting(actor, row, input, key), recordScope(row));
  };
}

export interface ExpensePaymentPorts extends ExpenseCommandPort {
  expense(actor: WorkContext, id: string): Promise<ExpenseRecord>;
  payers(actor: WorkContext, row: ExpenseRecord): Promise<string[]>;
  preparePayment(actor: WorkContext, key: string, id: string, input: { expectedVersion: number }, row: ExpenseRecord): Promise<ExpenseMutation>;
  payment(actor: WorkContext, id: string): Promise<{ expense: ExpenseRecord; draft: ExpenseRecord }>;
  payable(actor: WorkContext, row: ExpenseRecord): Promise<ExpenseRecord | null>;
  paymentEvidence(actor: WorkContext, id: string, expectedVersion: number, key: string): Promise<unknown>;
  hasPaymentSlip(id: string): Promise<boolean>;
  prepareConfirmation(actor: WorkContext, key: string, id: string, input: ExpensePaymentInput, expense: ExpenseRecord, draft: ExpenseRecord, payable: ExpenseRecord, evidence: unknown): Promise<ExpenseMutation>;
}
export function createExpensePaymentCommands(ports: ExpensePaymentPorts) {
  return {
    async createExpensePayment(actor: WorkContext, id: string, expectedVersion: number, key: string) {
      const input = { expectedVersion }, operation = "expense.payment.prepare";
      const prior = await ports.replay(actor, key, operation, id, input);
      if (prior) return prior;
      const row = await ports.expense(actor, id);
      if (row.workflow_version !== 2 || row.workflow_stage !== "payment" || !(await ports.payers(actor, row)).includes(actor.userId))
        throw ports.error({ status: 403, code: "TREASURY_REQUIRED", message: "Only an independent Treasury operator can record this payment." });
      return ports.commit(actor, key, operation, input, id, expectedVersion, await ports.preparePayment(actor, key, id, input, row));
    },
    async confirmExpensePayment(actor: WorkContext, id: string, input: ExpensePaymentInput, key: string, expenseId?: string) {
      const operation = "expense.payment.confirm", prior = await ports.replay(actor, key, operation, id, input);
      if (prior) return prior;
      const { expense, draft } = await ports.payment(actor, id);
      if (expenseId && expense.id !== expenseId)
        throw ports.error({ status: 404, code: "PAYMENT_DRAFT_NOT_FOUND", message: "Payment evidence does not belong to this claim." });
      const payable = await ports.payable(actor, expense);
      if (!payable) throw ports.error({ status: 409, code: "EXPENSE_PAYABLE_REQUIRED", message: "The approved expense payable is unavailable." });
      const evidence = await ports.paymentEvidence(actor, id, Number(draft.version), key);
      if (!evidence && !await ports.hasPaymentSlip(id))
        throw ports.error({ status: 422, code: "PAYMENT_SLIP_REQUIRED", message: "Attach the real bank payment slip before confirming." });
      const mutation = await ports.prepareConfirmation(actor, key, id, input, expense, draft, payable, evidence);
      return ports.commit(actor, key, operation, input, id, input.expectedVersion + 1, mutation, recordScope(expense));
    },
  };
}

export type ExpenseDraftFacts = {
  recordId: string; current: ExpenseRecord | null; claimant: string; claimantName: string; personId: string | number | null;
  staff: ExpenseStaff | null; amount: number | null; tax: number; category: string | null; currency: string | null;
  requestKind: string | number; payeePartyId: string | number | null;
};
export interface ExpenseDraftPorts extends ExpenseCommandPort {
  expense(actor: WorkContext, id: string): Promise<ExpenseRecord>;
  authority(actor: WorkContext): Promise<unknown>;
  claimant(actor: WorkContext, id: string): Promise<{ id: string; user_id: string | null; legal_name: string } | null>;
  staff(actor: WorkContext, claimant: string): Promise<ExpenseStaff | null>;
  latestEngagement(actor: WorkContext, personId: string | number | null): Promise<{ branch_id: string | null; department_id: string | null } | null>;
  canManagePeople(actor: WorkContext): boolean;
  money(value: string): number;
  prepareDraft(actor: WorkContext, key: string, input: ExpenseDraftInput, facts: ExpenseDraftFacts, authority: unknown): Promise<ExpenseMutation>;
}
export function createSaveExpenseDraft(ports: ExpenseDraftPorts) {
  return async (actor: WorkContext, id: string | null, input: ExpenseDraftInput, key: string) => {
    const recordId = id ?? `expense-${key}`, operation = id ? "expense.update" : "expense.create";
    const prior = await ports.replay(actor, key, operation, recordId, input);
    if (prior) return prior;
    const current = id ? await ports.expense(actor, id) : null, authority = await ports.authority(actor);
    if (current && !mayEditExpense(actor, current))
      throw ports.error({ status: 403, code: "EXPENSE_LOCKED", message: "Return the claim for correction before changing its details." });
    let claimant = String(current?.claimant_user_id ?? actor.userId), claimantName = String(current?.claimant_name ?? actor.name), personId = current?.employee_id ?? null;
    if (input.personId) {
      const person = await ports.claimant(actor, input.personId);
      if (!person?.user_id) throw ports.error({ status: 422, code: "CLAIMANT_UNAVAILABLE", message: "Choose a person with a linked company account." });
      if (current && person.user_id !== claimant) throw ports.error({ status: 422, code: "CLAIMANT_IMMUTABLE", message: "Create a separate claim for another person." });
      if (person.user_id !== actor.userId && !ports.canManagePeople(actor)) throw ports.error({ status: 403, code: "EXPENSE_OWNER_REQUIRED", message: "Create a claim for yourself." });
      claimant = person.user_id; claimantName = person.legal_name; personId = person.id;
    }
    const staff = await ports.staff(actor, claimant);
    if (claimant !== actor.userId && (actor.branchId || actor.departmentId)) {
      const personScope = staff ?? await ports.latestEngagement(actor, personId);
      if (!personScope || (actor.branchId && personScope.branch_id !== actor.branchId) || (actor.departmentId && personScope.department_id !== actor.departmentId))
        throw ports.error({ status: 404, code: "PERSON_NOT_FOUND", message: "Person not found in this workspace." });
    }
    if (staff) {
      if (claimant !== actor.userId && ((actor.branchId && staff.branch_id !== actor.branchId) || (actor.departmentId && staff.department_id !== actor.departmentId)))
        throw ports.error({ status: 404, code: "PERSON_NOT_FOUND", message: "Person not found in this workspace." });
      personId = staff.person_id; claimantName = staff.legal_name;
    }
    const amount = input.amount ? ports.money(input.amount) : null, tax = input.tax ? ports.money(input.tax) : 0;
    if (amount !== null && (amount <= 0 || !Number.isSafeInteger(amount + tax)))
      throw ports.error({ status: 422, code: "EXPENSE_AMOUNT_INVALID", message: "Use a positive amount within the supported precision." });
    const version = current ? input.expectedVersion + 1 : 1, requestKind = input.requestKind ?? current?.request_kind ?? "expense";
    const mutation = await ports.prepareDraft(actor, key, input, { recordId, current, claimant, claimantName, personId, staff, amount, tax, category: input.categoryAccountId || null, currency: input.currency || null, requestKind, payeePartyId: requestKind === "payment" ? (input.payeePartyId ?? current?.payee_party_id ?? null) : null }, authority);
    return ports.commit(actor, key, operation, input, recordId, version, mutation, { companyId: actor.companyId, branchId: staff?.branch_id ?? null, departmentId: staff?.department_id ?? null });
  };
}

function recordScope(row: ExpenseRecord): ExpenseScope {
  return { companyId: String(row.company_id), branchId: row.branch_id ? String(row.branch_id) : null, departmentId: row.department_id ? String(row.department_id) : null };
}

export type ExpenseTransitionInput = { expectedVersion: number; reason?: string; override?: boolean };
export type ExpenseTransitionAction = "submit" | "approve" | "reject" | "cancel";
export type ExpenseApprovalRoute = { blockedReason?: string | null; steps: { stage?: string }[]; native: unknown };
export type ExpenseApprovalDecision = { next?: { stage?: string | null } | null; native: unknown };
export interface ExpenseTransitionPorts extends ExpenseCommandPort {
  now(): number;
  requirements(row: ExpenseRecord): Promise<string[]>;
  staff(actor: WorkContext, row: ExpenseRecord): Promise<ExpenseStaff>;
  route(actor: WorkContext, staff: ExpenseStaff, row: ExpenseRecord): Promise<ExpenseApprovalRoute>;
  open(actor: WorkContext, key: string, id: string, input: ExpenseTransitionInput, staff: ExpenseStaff, route: ExpenseApprovalRoute): Promise<unknown>;
  decision(actor: WorkContext, key: string, id: string, action: ExpenseTransitionAction, input: ExpenseTransitionInput): Promise<ExpenseApprovalDecision>;
  prepareSubmit(actor: WorkContext, key: string, id: string, input: ExpenseTransitionInput, row: ExpenseRecord, staff: ExpenseStaff, route: ExpenseApprovalRoute, opened: unknown, now: number): Promise<ExpenseMutation>;
  prepareCancel(actor: WorkContext, key: string, id: string, input: ExpenseTransitionInput, row: ExpenseRecord, now: number): Promise<ExpenseMutation>;
  prepareGlReturn(actor: WorkContext, key: string, id: string, input: ExpenseTransitionInput, row: ExpenseRecord, now: number): Promise<ExpenseMutation>;
  prepareAccountingApproval(actor: WorkContext, key: string, id: string, input: ExpenseTransitionInput, row: ExpenseRecord, decision: ExpenseApprovalDecision, now: number): Promise<ExpenseMutation>;
  prepareDecision(actor: WorkContext, key: string, id: string, action: ExpenseTransitionAction, input: ExpenseTransitionInput, row: ExpenseRecord, decision: ExpenseApprovalDecision, stage: string, now: number): Promise<ExpenseMutation>;
}

/** Current workflow decisions are independent of SQL, runtime and effectful clocks. */
export function createTransitionExpenseV2(ports: ExpenseTransitionPorts) {
  return async (actor: WorkContext, row: ExpenseRecord, action: ExpenseTransitionAction, input: ExpenseTransitionInput, key: string) => {
    const id = String(row.id), operation = `expense.${action}`;
    const prior = await ports.replay(actor, key, operation, id, input);
    if (prior) return prior;
    const own = [row.claimant_user_id, row.created_by].some(id => samePerson(actor, id)), now = ports.now();
    if (action !== "submit" && (input.reason?.trim().length ?? 0) < 3)
      throw ports.error({ status: 422, code: "REASON_REQUIRED", message: "Record the reason for this decision." });
    if (action === "submit") {
      if (!own || !["draft", "rejected"].includes(String(row.status)) || row.cancelled_at)
        throw ports.error({ status: 403, code: "EXPENSE_OWNER_REQUIRED", message: "Only the claimant or preparer can submit an editable claim." });
      const missing = await ports.requirements(row);
      if (missing.length) throw ports.error({ status: 422, code: "EXPENSE_SETUP_REQUIRED", message: missing.join(" ") });
      const staff = await ports.staff(actor, row), route = await ports.route(actor, staff, row);
      if (route.blockedReason) throw ports.error({ status: 422, code: "EXPENSE_ROUTE_REQUIRED", message: route.blockedReason });
      const opened = await ports.open(actor, key, id, input, staff, route);
      return ports.commit(actor, key, operation, input, id, input.expectedVersion + 1, await ports.prepareSubmit(actor, key, id, input, row, staff, route, opened, now), { companyId: actor.companyId, branchId: staff.branch_id, departmentId: staff.department_id });
    }
    if (action === "cancel") {
      if (!own || !["draft", "rejected", "in_review"].includes(String(row.status)) || row.cancelled_at)
        throw ports.error({ status: 403, code: "EXPENSE_OWNER_REQUIRED", message: "Only the requester can cancel an unposted claim." });
      return ports.commit(actor, key, operation, input, id, input.expectedVersion + 1, await ports.prepareCancel(actor, key, id, input, row, now), recordScope(row));
    }
    if (row.workflow_stage === "gl_posting" && row.status === "approved" && action === "reject")
      return ports.commit(actor, key, operation, input, id, input.expectedVersion + 1, await ports.prepareGlReturn(actor, key, id, input, row, now), recordScope(row));
    if (row.status !== "in_review" || row.cancelled_at)
      throw ports.error({ status: 409, code: "EXPENSE_NOT_PENDING", message: "This claim is not awaiting a decision." });
    const decision = await ports.decision(actor, key, id, action, input);
    const accounting = JSON.parse(String(row.accounting_json ?? "{}")) as { periodId?: string; entryDate?: string; preparedBy?: string };
    if (action === "approve" && ["accounts_review", "accounts_approval"].includes(String(row.workflow_stage)) && (!accounting.periodId || !accounting.entryDate))
      throw ports.error({ status: 422, code: "ACCOUNTING_REQUIRED", message: "Prepare and review the category, tax and posting details first." });
    if (action === "approve" && row.workflow_stage === "accounts_approval") {
      if (accounting.preparedBy === actor.userId)
        throw ports.error({ status: 403, code: "INDEPENDENT_MANAGER_REQUIRED", message: "A different Accounts manager must approve the accounting preparation." });
      return ports.commit(actor, key, operation, input, id, input.expectedVersion + 1, await ports.prepareAccountingApproval(actor, key, id, input, row, decision, now), recordScope(row));
    }
    const stage = action === "reject" ? "draft" : decision.next?.stage ?? "accounts_approval";
    return ports.commit(actor, key, operation, input, id, input.expectedVersion + 1, await ports.prepareDecision(actor, key, id, action, input, row, decision, stage, now), recordScope(row));
  };
}
