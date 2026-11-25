import "server-only";
/** Canonical owner-local alias-qualified row head shared by the Work/Plan
 * source-image projection and the scoped source CTE in the package-obligations
 * facts read. Both compose byte-identical heads; callers pass only an owner-fixed
 * alias and a closed tail fragment, never request-authored SQL. Each composed
 * string stays byte-identical to its pre-dedupe literal and no LOCK/UPDATE
 * statement is accepted or changed. */
export const aliasRowsSql = (alias: string, tail: string): string => `SELECT ${alias}.*${tail}`;
