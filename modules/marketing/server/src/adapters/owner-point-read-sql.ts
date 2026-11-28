import { marketingSelectHeadSql } from './select-head-sql';

/** Closed owner-local id+company point-read head (r40, delegated in r65). The
 * attachment fact, attachment source-witness and record-purchases reads share
 * this one renderer: a caller picks an owner-owned projection, a table or
 * subquery source, and a qualifier flag. The selection head is the registered
 * owner head; every emitted statement keeps exactly two binds and its original
 * bytes, and no caller-authored SQL crosses this boundary. FOR SHARE locks,
 * command_receipts settlement and native read ports stay untouched. */
export function marketingOwnerPointReadSql(projection: string, source: string, qualify = false): string {
  const q = qualify ? 'source.' : '';
  return marketingSelectHeadSql(projection, ` FROM ${source} WHERE ${q}id=? AND ${q}company_id=?`);
}
