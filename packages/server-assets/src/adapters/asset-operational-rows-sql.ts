import "server-only";
/** Canonical owner-local fixed operational_assets projection. The field-service
 * picker rows and the closed final-root owner projection share this one literal
 * so the TS read tail carries no duplicate fixed-relation projection. Callers
 * pass only a fixed owner-owned column list; no SQL or relation name is accepted
 * from a request. */
export const assetOperationalRowsSql = (columns: string) => `SELECT ${columns} FROM operational_assets`;
