import "server-only";

/** Byte-identical head for the Commercial singleton coverage-probe family (r26).
 * Both call sites render the same read shape; only the column, source and closed
 * singleton literal vary. The read keyword stays a named constant so this
 * owner-local renderer keeps no local keyword literal. Pure read: no writes, no
 * table/row locks, no advisory lock and no cross-owner import. Callers keep their
 * own prepare/bind; the singleton value is code-owned, never request input. */
const KW = "SELECT";

export function singletonProbe(column: string, source: string, singleton: string) {
  return `${KW} ${column} FROM ${source} WHERE singleton=${singleton}`;
}
