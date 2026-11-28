/** Byte-identical Manufacturing read-tail r36 SQL text shared by the
 * accounting-profile, stock-footprint and package-settlement read heads. All
 * of them opened the same owner-table projection head up to `FROM
 * manufacturing_`, differing only in the trailing table/alias/join/predicate
 * text; one parameterized emitter renders each one byte for byte. Text only:
 * every caller still prepares and executes its own statement with its own
 * binds, result type and transaction; nothing is cached or executed here and
 * it stays outside the RD_* read registry. The r15-r35 helpers, every
 * lock-bearing and native witness and all cross-owner statements remain
 * untouched. */

export const manufacturingOwnerTableSelect = (columns: string, tail: string) =>
  `SELECT ${columns} FROM manufacturing_${tail}`;
