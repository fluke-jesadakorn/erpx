import "server-only";
/** Canonical owner-local fixed all-rows projection for Assets. The non-enforced
 * metric source and the package-settlement Finance-link source share this one
 * literal so the TS read tail carries no duplicate statement. Callers pass only
 * fixed owner-owned relation names; no SQL or relation name is accepted from a
 * request. */
export const assetAllRowsSql = (table: string) => `SELECT * FROM ${table}`;
