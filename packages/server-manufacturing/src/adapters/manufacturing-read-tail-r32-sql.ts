/** Byte-identical Manufacturing read-tail r32 SQL text shared by the two
 * executive metric breakdown projections. The completed-orders count and the
 * production-cost sum opened the same label/value projection head, with only
 * the aggregation expression and the FROM/WHERE/GROUP tail varying; one
 * parameterized emitter renders each one byte for byte. Each
 * caller still prepares and executes its own statement with its own binds and
 * result type; nothing is cached or executed here and it stays outside the
 * RD_* read registry. The r4/r11/r15-r31 helpers, the branch-display-label
 * fragment and all LOCK TABLE / FOR UPDATE witnesses remain untouched. */

export const manufacturingMetricBreakdownSelect = (label: string, value: string, tail: string) =>
  `SELECT ${label} label,${value} value${tail}`;
