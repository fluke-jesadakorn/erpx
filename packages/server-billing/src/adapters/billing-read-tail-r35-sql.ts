/** Byte-identical Billing read-tail r35 generic projection head. Text only:
 * every caller supplies its own projection columns and FROM tail and executes
 * with its own binds; nothing is cached or executed here. A tail beginning with
 * a newline owns its leading whitespace (r42), so no separator space is added. */
export const billingProjectionSql = (columns: string, tail: string) =>
  `SELECT ${columns}${tail.startsWith('\n') ? '' : ' '}${tail}`;
