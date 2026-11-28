/** G4 / retire.finance.read-tail-r48 safe NEW read-tail SQL families.
 *
 * Every export renders byte-identical text to the literal it replaces. Callers
 * keep their own binds, result types and native write pairing; nothing here is
 * cached or executed. Locks, tax, advisory, native, expansion unions and prior
 * r15-r47 helpers are untouched.
 */
import { PLAN_EDITION_POINT_SQL } from './finance-read-tail-r38-sql';

/** A plan edition point with the caller's extra predicates. */
export const planEditionPointSql = (extra: string) => PLAN_EDITION_POINT_SQL + extra;

/** Bank transaction existence guard with the caller's matched/unmatched clause. */
export const bankTxnGuardSql = (clause: string) =>
  `EXISTS(SELECT 1 FROM finance_bank_transactions WHERE id=? AND company_id=?${clause})`;

/** Acquisition read with the caller's projection and filter text. */
export const acquisitionReadSql = (projection: string, filters: string) =>
  `SELECT ${projection} FROM finance_acquisitions WHERE ${filters}`;

/** Acquisition version guard for a single status literal. */
export const acquisitionVersionGuardSql = (status: string) =>
  `EXISTS(SELECT 1 FROM finance_acquisitions WHERE id=? AND company_id=? AND holding_id=? AND status='${status}' AND version=?)`;
