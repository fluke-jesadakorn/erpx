import 'server-only';

/** Canonical parameterized read builders (G4 read-tail r56). Each builder owns one
 * byte-identical SQL head formerly repeated in inline literals at this owner's
 * call sites; the callers keep their original status lists, tails, binds and branches
 * unchanged. Pure reads only: no writes, row/table locks, advisory statements,
 * native kinds or cross-owner projection tails. */

/** Expense-claim version/status guard. `statuses` is the exact original status list —
 * a literal at one caller, the caller's ?-placeholder list at the other. The fixed
 * head, owner kind and bind arity stay byte-identical. */
export const ppRd56ExpenseClaimStatusGuard = (statuses: string) =>
  `EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND cancelled_at IS NULL AND status IN (${statuses}))`;

/** Current-engagement version guard. `tail` is the exact original predicate tail
 * (version + status list, or branch/department scope); the head stays byte-identical. */
export const ppRd56EngagementGuard = (tail: string) =>
  `EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND ${tail})`;

/** Posted payroll-run item row. `tail` is the exact original predicate/ordering tail;
 * the projection head and the caller's binds stay byte-identical. */
export const ppRd56PayrollItemRow = (tail: string) =>
  `SELECT id,engagement_id,net_minor,currency,input_snapshot_json FROM hr_payroll_items WHERE run_id=? AND ${tail}`;
