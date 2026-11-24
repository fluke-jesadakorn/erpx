import { marketingOwnerPointReadSql } from "./owner-point-read-sql";
/** Canonical owner-local fixed SQL for Marketing attachment source-witness reads.
 * The purchase-boundary witness and the attachment image reader share this one
 * admitted source projection, so no duplicate TS SQL literal remains. Callers
 * supply only owner-owned table/projection fragments, never caller-authored SQL. */
export const attachmentSourceWitnessSql = (projection: string, table: string): string =>
  marketingOwnerPointReadSql(projection, `${table} source`, true);
