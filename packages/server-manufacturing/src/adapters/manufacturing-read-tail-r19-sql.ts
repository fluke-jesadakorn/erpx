/** Byte-identical Manufacturing read-tail r19 SQL text shared across the
 * accounting-profile readers. Text only: every caller still prepares and
 * executes its own statement with its own columns, alias, predicate and binds;
 * nothing is cached or executed here and it stays outside the RD_* read
 * registry. An empty alias or predicate emits no extra whitespace. */

export const manufacturingAccountingProfileSelect = (
  columns: string,
  alias = "",
  where = "",
) =>
  `SELECT ${columns} FROM manufacturing_accounting_profiles${alias ? ` ${alias}` : ""}${where ? ` WHERE ${where}` : ""}`;
