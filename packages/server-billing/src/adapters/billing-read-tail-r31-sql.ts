/** Parameterized Billing read-tail r31 text shared by the target-month id/scope
 * reads. Text only: each caller supplies its columns, table and suffix and
 * executes its own statement with its own two binds; nothing is cached or
 * executed here. */
import {billingPointReadSql} from './billing-read-tail-r29-sql';

export const billingTargetMonthSql = (columns: string, table: string, tail: string) =>
  billingPointReadSql(columns, table, `holding_id=$1 AND month=$2${tail}`);
