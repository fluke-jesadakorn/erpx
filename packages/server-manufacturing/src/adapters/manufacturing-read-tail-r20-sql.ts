/** Byte-identical Manufacturing read-tail r20 SQL text shared across the
 * cross-module EXISTS guard witnesses. Text only: every caller still prepares
 * and executes its own assertion statement with its own table, predicate and
 * binds; nothing is cached or executed here and it stays outside the RD_* read
 * registry. Each argument pair re-emits the previously inline SQL byte for byte. */

export const manufacturingExistsGuard = (table: string, predicate: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE ${predicate})`;
