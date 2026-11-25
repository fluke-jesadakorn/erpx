/** Byte-identical Billing read-tail r29 point-read text. Text only: each caller
 * supplies its own columns, table and suffix and executes with its own binds.
 * r39: strict specialization of the r35 generic projection head. */
import { billingProjectionSql } from './billing-read-tail-r35-sql';
export const billingPointReadSql = (columns: string, table: string, suffix: string) =>
  billingProjectionSql(columns, `FROM ${table} WHERE ${suffix}`);
