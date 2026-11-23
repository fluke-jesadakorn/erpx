/** Canonical owner-local fixed SQL for Marketing attachment owner version
 * guards. The service and operational attachment fact sources share this one
 * admitted guard predicate, so no duplicate TS SQL literal remains. Callers
 * supply only owner-owned table/extra fragments, never caller-authored SQL. */
export const attachmentOwnerVersionGuardSql = (table: string, extra = ""): string =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?${extra ? ` AND ${extra}` : ""})`;
