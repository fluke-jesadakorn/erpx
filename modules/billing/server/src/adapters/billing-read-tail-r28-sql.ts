/** Parameterized Billing read-tail r28 SQL text shared by the evidence storage
 * ledger point read and the stored-totals projections. Text only: every caller
 * supplies its own projection columns and predicate and executes its own
 * statement with its own binds; nothing is cached or executed here. */
import {billingPointReadSql} from './billing-read-tail-r29-sql';

export const billingStoredLedgerSql = (columns: string, where: string) =>
  billingPointReadSql(columns, 'billing_evidence_storage_ledger', where);
