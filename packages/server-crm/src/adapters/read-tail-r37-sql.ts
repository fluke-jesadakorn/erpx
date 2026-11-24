/** G4 / retire.crm.read-tail-r37 byte-identical read heads.
 *
 * Three owner-local read families share one renderer each: the two single
 * company-key source reads, the two executive metric breakdown projections and
 * the two qualified Sales opportunity reads. Callers pass their own projection,
 * source, aggregation and tail so rendered SQL and bind order stay byte-identical
 * to the retired inline literals. Locks, command-receipt reads, native command
 * kinds and the r8-r36 families are untouched.
 */

/** Single company-key source read. The lead application uses it for the company
 * currency and for the distinct owner-id set inside the recipient read; both
 * pass their own projection/alias/tail so the rendered text is unchanged. */
export const crmSingleKeySourceSql = (
  projection: string,
  source: string,
  alias: string,
  tail = '',
) => `SELECT ${projection} FROM (${source}) ${alias} WHERE company_id=?${tail}`;

/** Executive metric breakdown head. Callers pass the label expression, the
 * aggregation expression and everything after the value alias; the one read
 * token lives here and renders both former literals byte-for-byte. */
export const crmMetricBreakdownSql = (label: string, value: string, tail: string) =>
  `SELECT ${label} label,${value} value${tail}`;

/** Qualified Sales opportunity read head. Callers pass the projection, the
 * lead/opportunity join and the full predicate/tail; rendered SQL and bind order
 * stay byte-identical to the retired inline statements. */
export const crmSalesOpportunitySql = (projection: string, from: string, tail: string) =>
  `SELECT ${projection} FROM ${from} WHERE ${tail}`;
