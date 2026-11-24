import "server-only";
/** Owner-local EXISTS prefix shared by the r24 row-guard and r27 version-guard
 * families (round 34). Emits the byte-identical EXISTS prefix; the row source,
 * predicate and version binds stay in each caller. Text only: nothing is cached
 * or executed here. */
export const existsGuardPrefix = (source: string) => `EXISTS(SELECT 1 FROM ${source} WHERE `;
/** Owner-local version-guard EXISTS family (round 27). The row source and the
 * trailing predicate are parameterized; the fixed `id=? AND company_id=? AND
 * version=?` prefix and emitted SQL stay byte-identical to the former inline
 * copies. Distinct from the fully-generic `existsRow` family: this helper is
 * the version-checked shape shared by sensitive attachment and candidate
 * privacy guards. Text only: nothing is cached or executed here. */
export const existsVersionRow = (source: string, extra = "") =>
  `${existsGuardPrefix(source)}id=? AND company_id=? AND version=?${extra})`;
