/** Procurement read-tail r27 shared fragment. Read-only text only: the builder
 * renders the original statement so binds, ordering, locks and writes are
 * unchanged. The r17-r26 helpers stay untouched. */
/** Company-keyed settings read. `columns` is the caller's exact projection and
 * `settings` the owner projection source. No binds, locks or writes introduced. */
export const settingsRead = (columns: string, settings: string, tail = "") =>
  `SELECT ${columns} FROM (${settings}) commerce_settings WHERE company_id=?${tail}`;
