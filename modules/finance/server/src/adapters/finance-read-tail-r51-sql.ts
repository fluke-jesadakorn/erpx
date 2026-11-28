/** G4 / retire.finance.read-tail-r51 safe NEW read-tail SQL family.
 *
 * Every export renders byte-identical text to the literal it replaces. Callers
 * keep their own binds, result types and transaction; nothing here is cached or
 * executed. Row-lock, tax, assertion, advisory/native/write guards, r42 ordered
 * lists and r15-r50 helpers remain untouched.
 */

/** Company-scoped active account list; callers supply projection and order text. */
export const activeAccountListSql = (projection: string, orderBy: string) =>
  `SELECT ${projection} FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY ${orderBy}`;
