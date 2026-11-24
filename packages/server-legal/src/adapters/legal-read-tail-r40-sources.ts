import "server-only";
/** Byte-identical Legal EXISTS point-guard head family shared by owner adapters (round 40).
 * Text only: every call site still prepares, binds and executes its own read with unchanged binds.
 * Callers pass trusted constant table/key identifiers; no request text is interpolated. */
export const legalExistsPointGuardHeadSql = (table: string, key: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE ${key}=? AND `;
