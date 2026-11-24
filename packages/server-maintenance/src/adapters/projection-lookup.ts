import "server-only";
/** Canonical owner-local row-projection head shared by the scalar lookup and
 * the top-level multi-row scan. Callers pass only owner-fixed field, source,
 * alias and tail fragments, never request-authored SQL; each composed string
 * stays byte-identical to its pre-dedupe literal. No LOCK statement is accepted
 * or changed. */
export const projectionHead = (fields: string, source: string, alias: string, tail: string): string =>
  `SELECT ${fields} FROM (${source}) ${alias} ${tail}`;
/** Canonical owner-local scalar projection lookup shared by the Calendar
 * owner-membership witness, the executive metric label and the package-
 * obligation asset-version read. It reuses the single row-projection head so no
 * second query head is carried in the TS read tail. Callers pass only owner-fixed
 * projection, select, alias and predicate fragments, never request-authored SQL;
 * each composed string stays byte-identical to its pre-dedupe literal. No LOCK
 * statement is accepted or changed. */
export const projectionLookup = (select: string, source: string, alias: string, where: string): string =>
  `(${projectionHead(select, source, alias, `WHERE ${where}`)})`;
/** Canonical owner-local EXISTS witness head shared by the attachment owner
 * version guard, the IAM scope guard and the package-settlement upstream-source
 * witness. Callers pass only owner-fixed source/alias and predicate fragments;
 * each composed guard stays byte-identical to its pre-dedupe literal. No LOCK
 * statement is accepted or changed. */
export const existsGuardPrefix = (source: string): string => `EXISTS(SELECT 1 FROM ${source} WHERE `;
/** Negated head over the same owner-local EXISTS witness. The Work assets read
 * asserts that no active, non-terminal Work row references an asset; it reuses
 * the single existsGuardPrefix head so no second query head is carried in the
 * TS read tail. The composed guard stays byte-identical to its pre-dedupe
 * literal. No LOCK statement is accepted or changed. */
export const notExistsGuardPrefix = (source: string): string => `NOT ${existsGuardPrefix(source)}`;
