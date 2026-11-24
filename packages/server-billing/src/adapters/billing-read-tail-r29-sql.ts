/** Byte-identical Billing read-tail r29 point-read text. Text only: each caller
 * supplies its own columns, table and suffix and executes with its own binds. */
export const billingPointReadSql = (columns: string, table: string, suffix: string) =>
  `SELECT ${columns} FROM ${table} WHERE ${suffix}`;
