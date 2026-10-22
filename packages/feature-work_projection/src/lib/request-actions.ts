/** Canonical request commands shared by Start, New, shortcuts and direct URLs. */
export const requestActions = {
  expense: { id: "expense", label: "Expense claim", path: "/expenses/new", owner: "people_payroll", objective: "Attach a receipt and request reimbursement for a business expense." },
  leave: { id: "leave", label: "Leave request", path: "/leave/new", owner: "people_payroll", objective: "Review your eligibility, dates and handover before requesting leave." },
  purchase_request: { id: "buy", label: "Purchase request", path: "/requests/new?kind=purchase", owner: "operations", objective: "Describe what you need and an estimated cost for independent purchasing review." },
  service_request: { id: "help", label: "Service request", path: "/requests/new?kind=service", owner: "operations", objective: "Describe the problem and affected people, then confirm the responsible team." },
} as const;
export type RequestActionKind = keyof typeof requestActions;
export function requestAction(kind: string) {
  return Object.hasOwn(requestActions,kind) ? requestActions[kind as RequestActionKind] : null;
}
