/** Parameterized Billing read-tail r30 text shared by the whole-account column
 * projections. Text only: every caller supplies its own projection columns and
 * executes its own statement with its own binds; nothing is cached or executed
 * here. */
import {billingProjectionSql} from './billing-read-tail-r35-sql';

export const billingAccountProjectionSql = (columns: string) =>
  billingProjectionSql(columns, 'FROM billing_accounts');
