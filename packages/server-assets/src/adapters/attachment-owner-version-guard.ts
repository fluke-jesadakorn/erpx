import {assetSourceProbeSql} from "./record-purchases";
/** Canonical owner-local fixed SQL for Assets attachment owner version
 * guards. The service and operational attachment fact sources share this one
 * admitted guard predicate, so no duplicate TS SQL literal remains. The row
 * probe is the owner-local `assetSourceProbeSql` family; callers supply only
 * owner-owned table/extra fragments, never caller-authored SQL. */
export const attachmentOwnerVersionGuardSql = (table: string, extra = ""): string =>
  `EXISTS(${assetSourceProbeSql(table, `id=? AND company_id=? AND version=?${extra ? ` AND ${extra}` : ""}`)})`;
