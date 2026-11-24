import 'server-only';
import { hospitalityOwnerProjectionSources as financeProjectionSources } from '@erpx/server-finance/public/hospitality-owner-projection-sources';

/** Canonical parameterized SELECT-only owner read builders shared across People Payroll
 * adapters (G4 read-tail r48). Each builder owns one read-only literal copied verbatim
 * from its former inline copies; consumers import it and keep no local copy. Every
 * family here is a pure read (no INSERT/UPDATE/DELETE, LOCK TABLE, FOR UPDATE/SHARE
 * or advisory lock) and is not covered by the r1-r47 canonical modules. Lock-bearing
 * reads, native kinds, cross-owner projections and r1-r47 modules stay untouched. */

/** Finance expense version guard. The owner table and the id/company/version head are
 * fixed owner fragments; each caller supplies its literal tail and keeps its three
 * binds (id, company_id, version) unchanged. */
export const ppRd48FinanceExpenseVersionGuard = (tail: string) =>
  `EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND ${tail})`;

/** By-id/by-company/by-version two-predicate existence guard. The owner table and the
 * id/company/version head are fixed owner fragments; both predicates are literal owner
 * fragments and each caller keeps its three binds unchanged. */
export const ppRd48IdCompanyVersionPairGuard = (table: string, predA: string, predB: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=? AND ${predA} AND ${predB})`;

/** Expense-claim invoice point read. The finance invoice projection is consumed verbatim
 * from the cross-owner projection source; each caller keeps its three binds
 * (invoice_id, company_id, source_id) unchanged. */
export const ppRd48ExpenseClaimInvoice = () =>
  `SELECT * FROM (${financeProjectionSources.finance_invoices}) finance_invoices WHERE id=? AND company_id=? AND source_type='expense_claim' AND source_id=?`;
