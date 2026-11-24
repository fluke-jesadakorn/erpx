import "server-only";
/** Canonical owner-local scalar projection lookup shared by the Calendar
 * owner-membership witness, the executive metric label and the package-
 * obligation asset-version read. Callers pass only owner-fixed projection,
 * select, alias and predicate fragments, never request-authored SQL; each
 * composed string stays byte-identical to its pre-dedupe literal. No LOCK
 * statement is accepted or changed. */
export const projectionLookup = (select: string, source: string, alias: string, where: string): string =>
  `(SELECT ${select} FROM (${source}) ${alias} WHERE ${where})`;
