/** Parameterized Billing read-tail r19 SQL text shared across owner-local readers.
 * Text only: every caller still prepares and executes its own statement with its
 * own binds and result-type alias; nothing is cached or executed here. */

/** Target provider-cost point reads share a holding/month prefix; each caller
 * appends its own predicate (for example the source-key guard). */
export function billingProviderCostsSql(extra = "") {
  return `SELECT * FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2${extra}`;
}

/** Usage-snapshot point reads share the native commercial source and the
 * holding/month predicate; each caller selects its own projection columns. */
export function billingUsageSnapshotSql(columns: string, source: string) {
  return `SELECT ${columns} FROM (${source}) billing_usage_snapshots WHERE holding_id=$1 AND month=$2`;
}
