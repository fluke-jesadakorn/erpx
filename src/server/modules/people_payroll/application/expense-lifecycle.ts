import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { samePerson } from "@/lib/identity_access/iam";
import { hasPermission } from "@/lib/identity_access/permissions";
import { minorToDecimal } from "@/lib/finance/finance-money";
import { expensePostingViolation, mayEditExpense, mayReviewExpense } from "../domain/expense-policy";
import type { ExpenseCommandPort, ExpenseMutation, ExpensePostingInput, ExpenseRecord, ExpenseTransitionAction, ExpenseTransitionInput } from "./expense-commands";

type LegacyRoute = { scope: { company_id: string; branch_id: string | null; department_id: string | null }; native: unknown };
type LegacyDecision = { final: boolean; native: unknown };
export interface ExpenseLifecyclePorts extends ExpenseCommandPort {
  expense(actor: WorkContext, id: string, override: boolean): Promise<ExpenseRecord | null>;
  current(actor: WorkContext, row: ExpenseRecord, action: ExpenseTransitionAction, input: ExpenseTransitionInput, key: string): Promise<Record<string, unknown>>;
  canOverride(actor: WorkContext, id: string): Promise<boolean>;
  decision(actor: WorkContext, id: string, action: ExpenseTransitionAction, input: ExpenseTransitionInput, key: string): Promise<LegacyDecision>;
  prepareLegacy(actor: WorkContext, id: string, row: ExpenseRecord, action: ExpenseTransitionAction, input: ExpenseTransitionInput, key: string, decision: LegacyDecision | null, route: LegacyRoute | null, opened: unknown, status: string): Promise<ExpenseMutation>;
}
export function createTransitionExpense(ports: ExpenseLifecyclePorts) {
  return async (actor: WorkContext, id: string, action: ExpenseTransitionAction, input: ExpenseTransitionInput, key: string) => {
    const operation = `expense.${action}`, prior = await ports.replay(actor, key, operation, id, input);
    if (prior) return prior;
    const row = await ports.expense(actor, id, Boolean(input.override)), review = action === "approve" || action === "reject";
    if (!row) throw ports.error({ status: 404, code: "NOT_FOUND", message: "Expense not found." });
    if (row.workflow_version === 2 || action === "submit") return ports.current(actor, row, action, input, key);
    if (!review && !(action === "cancel" ? [row.claimant_user_id, row.created_by].some(id => samePerson(actor, id)) && ["draft", "rejected", "in_review"].includes(String(row.status)) : mayEditExpense(actor, row)))
      throw ports.error({ status: 403, code: "EXPENSE_OWNER_REQUIRED", message: "Only the requester or preparer can submit or cancel this claim." });
    if (!input.reason?.trim()) throw ports.error({ status: 422, code: "REVIEW_REASON_REQUIRED", message: "Record the reason for this decision." });
    if (review && !(input.override ? await ports.canOverride(actor, id) : mayReviewExpense(actor, row)))
      throw ports.error({ status: 403, code: "CURRENT_APPROVER_REQUIRED", message: "Only the independent named approver can review this claim." });
    const decision = review ? await ports.decision(actor, id, action, input, key) : null;
    // Every submit is dispatched to the current workflow above; legacy reviews
    // retain their saved approval cycle and never rebuild a reporting chain.
    const status = action === "approve" ? decision?.final ? "approved" : "in_review" : "rejected";
    return ports.commit(actor, key, operation, input, id, input.expectedVersion + 1, await ports.prepareLegacy(actor, id, row, action, input, key, decision, null, null, status), scope(row));
  };
}

export type ExpenseJournalInput = { chartId: string; periodId: string; entryDate: string; currency: string; memo: string; sourceType: string; sourceId: string; isOpening: boolean; lines: { accountId: string; debit: string; credit: string }[] };
export type ExpensePreparedJournal = { id: string; native: unknown };
export type ExpensePostingResult = { id: string; status: string; version: number; invoiceId: string; journalId: string };
export interface ExpensePostingPreparationPorts {
  error: ExpenseCommandPort["error"];
  mapping(actor: WorkContext, claim: ExpenseRecord): Promise<ExpenseRecord | null>;
  journal(actor: WorkContext, input: ExpenseJournalInput, key: string): Promise<ExpensePreparedJournal>;
  preparePosting(actor: WorkContext, claim: ExpenseRecord, input: ExpensePostingInput, key: string, config: ExpenseRecord, total: number, invoice: string, payee: string, journal: ExpensePreparedJournal): Promise<ExpenseMutation<ExpensePostingResult>>;
}
export function createPrepareExpensePosting(ports: ExpensePostingPreparationPorts) {
  return async (actor: WorkContext, claim: ExpenseRecord, input: ExpensePostingInput, key: string) => {
    const violation = expensePostingViolation(actor, claim, input.entryDate);
    if (violation) throw ports.error(violation);
    const config = await ports.mapping(actor, claim);
    if (!config) throw ports.error({ status: 422, code: "EXPENSE_MAPPING_REQUIRED", message: "Configure matching expense, tax and payable accounts for this currency." });
    const id = String(claim.id), total = Number(claim.amount_minor) + Number(claim.tax_minor), invoice = `expense-invoice-${id}`, payee = claim.request_kind === "payment" && claim.payee_party_id ? String(claim.payee_party_id) : `expense-payee-${claim.employee_id}`;
    const lines = [
      { accountId: String(claim.category_account_id), debit: minorToDecimal(Number(claim.amount_minor)), credit: "0" },
      { accountId: String(config.payable_account_id), debit: "0", credit: minorToDecimal(total) },
    ];
    if (Number(claim.tax_minor)) lines.push({ accountId: String(config.input_tax_account_id), debit: minorToDecimal(Number(claim.tax_minor)), credit: "0" });
    const journal = await ports.journal(actor, { chartId: String(config.chart_id), periodId: input.periodId, entryDate: input.entryDate, currency: String(claim.currency), memo: `Expense reimbursement · ${claim.legal_name} · ${claim.merchant}`, sourceType: "expense_claim", sourceId: id, isOpening: false, lines }, key);
    return ports.preparePosting(actor, claim, input, key, config, total, invoice, payee, journal);
  };
}

export type ExpenseReversalInput = ExpensePostingInput & { reason: string };
export interface ExpenseReversalPorts extends ExpenseCommandPort {
  authorize(actor: WorkContext): WorkContext;
  expense(actor: WorkContext, id: string): Promise<ExpenseRecord>;
  invoice(actor: WorkContext, id: string, claim: ExpenseRecord): Promise<ExpenseRecord | null>;
  journal(actor: WorkContext, id: string, claim: ExpenseRecord): Promise<ExpenseRecord | null>;
  prepareReversal(actor: WorkContext, id: string, input: ExpenseReversalInput, key: string, claim: ExpenseRecord, invoice: ExpenseRecord, journal: ExpenseRecord): Promise<ExpenseMutation>;
}
export function authorizeExpenseReversal(actor: WorkContext, ports: Pick<ExpenseReversalPorts, "authorize" | "error">) {
  actor = ports.authorize(actor);
  if (!hasPermission(actor, "expense.approve") || actor.branchId || actor.departmentId) throw ports.error({ status: 403, code: "FINANCE_APPROVER_REQUIRED", message: "A holding-level finance approver must reverse posted expenses." });
  return actor;
}
export function createReverseExpense(ports: ExpenseReversalPorts) {
  return async (actor: WorkContext, id: string, input: ExpenseReversalInput, key: string) => {
    actor = authorizeExpenseReversal(actor, ports);
    const prior = await ports.replay(actor, key, "expense.reverse", id, input);
    if (prior) return prior;
    const claim = await ports.expense(actor, id);
    if ([claim.created_by, claim.prepared_by, claim.claimant_user_id].includes(actor.userId)) throw ports.error({ status: 403, code: "SELF_REVERSAL_FORBIDDEN", message: "A different finance approver must reverse this expense." });
    const invoice = await ports.invoice(actor, id, claim);
    if (!invoice || claim.status !== "posted" || invoice.status !== "posted" || invoice.outstanding_minor !== invoice.total_minor) throw ports.error({ status: 409, code: "EXPENSE_HAS_SETTLEMENT", message: "Only a posted expense with no recorded reimbursement can be reversed." });
    if (input.entryDate < String(invoice.invoice_date)) throw ports.error({ status: 422, code: "REVERSAL_DATE_INVALID", message: "Reversal date cannot precede the original posting." });
    const journal = await ports.journal(actor, id, claim);
    if (!journal) throw ports.error({ status: 409, code: "EXPENSE_JOURNAL_INVALID", message: "The expense posting is no longer available for reversal." });
    return ports.commit(actor, key, "expense.reverse", input, id, input.expectedVersion + 1, await ports.prepareReversal(actor, id, input, key, claim, invoice, journal), scope(claim));
  };
}
function scope(row: ExpenseRecord) { return { companyId: String(row.company_id), branchId: row.branch_id ? String(row.branch_id) : null, departmentId: row.department_id ? String(row.department_id) : null }; }
