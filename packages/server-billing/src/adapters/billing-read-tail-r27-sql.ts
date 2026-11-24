/** Parameterized Billing read-tail r27 SQL text shared by the executive metric
 * count projections. Text only: every caller supplies its own label and source
 * and binds/executes its own statement; nothing is cached or executed here. */
export const billingMetricCountSql = (label: string) =>
  `SELECT ${label} label,count(*)::text value FROM `;
