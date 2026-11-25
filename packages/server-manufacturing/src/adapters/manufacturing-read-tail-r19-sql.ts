/** Byte-identical Manufacturing read-tail r19 SQL text shared across the
 * accounting-profile readers. Text only: every caller still prepares and
 * executes its own statement with its own columns, alias, predicate and binds;
 * nothing is cached or executed here and it stays outside the RD_* read
 * registry. An empty alias or predicate emits no extra whitespace. The shared
 * owner-table head is delegated to the r36 emitter. */
import {manufacturingOwnerTableSelect} from './manufacturing-read-tail-r36-sql';

export const manufacturingAccountingProfileSelect = (
  columns: string,
  alias = "",
  where = "",
) =>
  manufacturingOwnerTableSelect(columns, `accounting_profiles${alias ? ` ${alias}` : ""}${where ? ` WHERE ${where}` : ""}`);
