/** Closed owner-local id+company point-read head (r40). The attachment fact,
 * attachment source-witness, campaign-existence and record-purchases reads share
 * this one renderer: a caller picks an owner-owned projection, a table or
 * subquery source, and a qualifier flag. Every emitted statement keeps exactly
 * two binds and its original bytes; no caller-authored SQL crosses this
 * boundary. FOR SHARE locks, command_receipts settlement and native read ports
 * stay untouched. */
export function marketingOwnerPointReadSql(projection: string, source: string, qualify = false): string {
  const q = qualify ? 'source.' : '';
  return `SELECT ${projection} FROM ${source} WHERE ${q}id=? AND ${q}company_id=?`;
}
