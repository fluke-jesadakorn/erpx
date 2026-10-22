import { mayEditExpense, type ExpenseActorFacts, type ExpenseRecordFacts, type ExpenseViolation } from "@erpx/server-people_payroll/domain/expense-policy";

export type ExpensePreviewActor = ExpenseActorFacts & {
  companyId: string; branchId: string | null; departmentId: string | null;
};
export type ExpensePreviewInput = { personId?: string | null; requestKind?: "expense" | "payment" };
export interface ExpensePreviewPorts<Actor extends ExpensePreviewActor, Route> {
  options(actor: Actor): Promise<{ people: ExpenseRecordFacts[] }>;
  expense(actor: Actor, id: string): Promise<ExpenseRecordFacts>;
  staff(actor: Actor, claimant: string): Promise<{ branch_id: string | null; department_id: string | null }>;
  route(actor: Actor, claimant: string, preparers: string[], requestKind: "expense" | "payment"): Promise<Route>;
  error(violation: ExpenseViolation): Error;
  blocker(error: unknown): string | null;
}

/** Claimant selection and scope rules are independent of Next and persistence. */
export function createExpensePreview<Actor extends ExpensePreviewActor, Route>(ports: ExpensePreviewPorts<Actor, Route>) {
  return async (actor: Actor, input: ExpensePreviewInput, currentId?: string): Promise<Route | { steps: never[]; blockers: string[] }> => {
    const options = await ports.options(actor);
    const current = currentId ? await ports.expense(actor, currentId) : null;
    if (current && !mayEditExpense(actor, current))
      throw ports.error({ status: 403, code: "EXPENSE_LOCKED", message: "Only an unsubmitted claim can be previewed." });
    const person = input.personId ? options.people.find(person => person.id === input.personId) : null;
    if (input.personId && !person)
      throw ports.error({ status: 404, code: "PERSON_NOT_FOUND", message: "Claimant not found in this workspace." });
    const claimant = String(current?.claimant_user_id ?? person?.user_id ?? actor.userId);
    if (current && person && person.user_id !== claimant)
      throw ports.error({ status: 422, code: "CLAIMANT_IMMUTABLE", message: "Create a separate claim for another person." });
    try {
      const staff = await ports.staff(actor, claimant);
      if ((actor.branchId && staff.branch_id !== actor.branchId) || (actor.departmentId && staff.department_id !== actor.departmentId))
        throw ports.error({ status: 404, code: "PERSON_NOT_FOUND", message: "Claimant not found in this workspace." });
      return await ports.route(actor, claimant, [String(current?.prepared_by ?? current?.created_by ?? actor.userId)], input.requestKind ?? "expense");
    } catch (error) {
      const blocker = ports.blocker(error);
      if (blocker !== null) return { steps: [], blockers: [blocker] };
      throw error;
    }
  };
}
