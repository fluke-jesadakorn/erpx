import "server-only";
/** Commercial read-tail r6 shared read-only SQL fragments. Each builder renders
 * the exact original literal so binds, ordering and lock/write behavior stay
 * unchanged; every consumer still prepares and binds its own independent
 * statement and keeps its exports. Arguments are code-owned column/table/field
 * fragments only, never request input. The r1/r3/r4/r5 helpers stay untouched.
 * Pure read: no writes, no LOCK TABLE, no FOR UPDATE/SHARE and no advisory lock. */

import { monthScopedRead } from "./commercial-read-tail-r13-sources";

/** Usage-snapshot header read keyed by billing month. */
export function snapshotByMonthSql(selectList: string) {
  return monthScopedRead(selectList, "billing_usage_snapshots");
}

/** Purchase shadow-observation aggregate over a Company grant/history source. */
export function purchaseObservationAgg(company: string, source: string, alias: string, fields: string) {
  return `COALESCE((SELECT jsonb_agg(jsonb_build_object(${fields})) FROM ${source} ${alias} WHERE ${alias}.company_id=${company}),'[]'::jsonb)`;
}
