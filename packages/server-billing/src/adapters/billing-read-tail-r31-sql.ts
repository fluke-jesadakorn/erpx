/** Parameterized Billing read-tail r31 text shared by the target-month id/scope
 * reads. Text only: each caller supplies its columns, table and suffix and
 * executes its own statement with its own two binds; nothing is cached or
 * executed here. */
export const billingTargetMonthSql = (columns: string, table: string, tail: string) =>
  `SELECT ${columns} FROM ${table} WHERE holding_id=$1 AND month=$2${tail}`;
