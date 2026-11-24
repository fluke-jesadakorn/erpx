import "server-only";
import {planningCommandExistsGuard} from "./planning-command-guard-sql";
/** Owner-local SQL fragments shared by the Planning attachment target readers.
 * Every argument is a fixed server-owned identifier or predicate fragment; no
 * bind value or caller SQL is interpolated. Each emitted statement is
 * byte-identical to the former inline literal it replaces. */
export const planningAttachmentTargetRow = (columns: string, table: string, extra = "") =>
  `SELECT company_id,${columns} FROM ${table} WHERE id=? AND company_id=?${extra}`;
/** r21 attachment-target version guard kept as a thin wrapper; the generalized
 * EXISTS builder owns the single read literal and emits the identical guard. */
export const planningAttachmentTargetVersionExists = (table: string, extra = "") =>
  planningCommandExistsGuard(table, `id=? AND company_id=? AND version=?${extra ? ` AND ${extra}` : ""}`);
