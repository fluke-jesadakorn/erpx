export const expenseStages = [
  "draft",
  "department_approval",
  "accounts_review",
  "accounts_approval",
  "gl_posting",
  "payment",
  "completed",
  "cancelled",
] as const;
export type ExpenseStage = (typeof expenseStages)[number];
export const expenseStageLabels: Record<ExpenseStage, string> = {
  draft: "Prepare claim",
  department_approval: "Department approval",
  accounts_review: "Accounts review",
  accounts_approval: "Accounts manager approval",
  gl_posting: "Post general ledger",
  payment: "Finance payment",
  completed: "Completed",
  cancelled: "Cancelled",
};
