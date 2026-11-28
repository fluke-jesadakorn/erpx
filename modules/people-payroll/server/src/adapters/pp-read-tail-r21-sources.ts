import 'server-only';
import { packageClockSql } from '@erpx/commercial-server/public/purchased-capability-sql';
/** Canonical byte-identical SELECT-only owner fragments shared across People Payroll
 * adapters (G4 read-tail r21). Each constant/builder owns one literal copied verbatim
 * from its former inline copies; consumers import it and keep no local copy. All are
 * pure reads: no INSERT, UPDATE, DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock. */
export const PP_RD21_CLOCK_NOW_MS_SQL = `(SELECT ${packageClockSql} now_ms)`;
export const PP_RD21_SCOPE_COMPANY_SQL = `WITH scope AS(SELECT ?::text company_id`;
export const ppRd21IdCompanyStatusVersionGuard = (table: string, status: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND status='${status}' AND version=?)`;
export const ppRd21IdVersionStatusGuard = (table: string, status: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND version=? AND status='${status}')`;
