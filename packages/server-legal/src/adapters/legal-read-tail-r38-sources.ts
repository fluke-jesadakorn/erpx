import "server-only";
/** Byte-identical Legal read fragment families shared by owner adapters (round 38).
 * Text only: every call site still prepares, binds and executes its own read with unchanged binds.
 * Callers pass trusted constant columns/tables/aliases/keys; no request text is interpolated. */
/** Released-item point guard; the caller supplies the fixed Company/version key. */
export const legalReleasedItemGuardSql = (key: 'company_id' | 'version') =>
  `SELECT 1 FROM legal_items i WHERE i.id=? AND i.${key}=?`;
/** Member edit point witness; the caller supplies a bound placeholder or a fixed source alias. */
export const legalMemberEditPointSql = (item: string) =>
  `SELECT 1 FROM legal_members WHERE item_id=${item} AND user_id=? AND access='edit'`;
/** Fact-table data read; the caller supplies the trusted fact table and fixed item reference. */
export const legalFactDataSql = (table: string, ref: string) =>
  `SELECT data FROM ${table} WHERE item_id=${ref}`;
/** Metric count projection prefix; the caller supplies the label expression and counted id. */
export const legalMetricCountPrefix = (label: string, id: string) =>
  `SELECT ${label} label,count(DISTINCT ${id})::text value`;
