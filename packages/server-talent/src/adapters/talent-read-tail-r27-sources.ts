import "server-only";
/** Owner-local version-guard EXISTS family (round 27). The row source and the
 * trailing predicate are parameterized; the fixed `id=? AND company_id=? AND
 * version=?` prefix and emitted SQL stay byte-identical to the former inline
 * copies. Distinct from the fully-generic `existsRow` family: this helper is
 * the version-checked shape shared by sensitive attachment and candidate
 * privacy guards. Text only: nothing is cached or executed here. */
export const existsVersionRow = (source: string, extra = "") =>
  `EXISTS(SELECT 1 FROM ${source} WHERE id=? AND company_id=? AND version=?${extra})`;
