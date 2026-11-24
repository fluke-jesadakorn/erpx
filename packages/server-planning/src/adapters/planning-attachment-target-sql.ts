import "server-only";
/** Owner-local SQL fragments shared by the Planning attachment target readers.
 * Every argument is a fixed server-owned identifier or predicate fragment; no
 * bind value or caller SQL is interpolated. Each emitted statement is
 * byte-identical to the former inline literal it replaces. */
export const planningAttachmentTargetRow = (columns: string, table: string, extra = "") =>
  `SELECT company_id,${columns} FROM ${table} WHERE id=? AND company_id=?${extra}`;
export const planningAttachmentTargetVersionExists = (table: string, extra = "") =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?${extra ? ` AND ${extra}` : ""})`;
