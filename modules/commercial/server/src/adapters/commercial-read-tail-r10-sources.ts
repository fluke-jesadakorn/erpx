import "server-only";
/** Commercial read-tail r10 parameterized EXISTS package-source probe family.
 * The builder owns one SQL literal and renders each original EXISTS subquery
 * byte-identically; every caller keeps its own prepare/bind and no
 * export changes. Arguments are code-owned source/alias/where fragments only,
 * never request input. The r1-r9 helpers, advisory/row locks and native owner
 * statement kinds stay untouched. Pure read-only subquery: no writes, no row
 * locks and no advisory lock. */
export function packageSourceProbe(source: string, alias: string, where: string, versionPredicate: string, tail: string) {
  return `EXISTS(SELECT 1 FROM ${source} ${alias} WHERE ${where}${versionPredicate}${tail})`;
}
