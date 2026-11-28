import 'server-only';
/** Byte-identical scope head shared by the metric-history and startup-entry
 * read tails. Both callers keep their own tail fragment, so rendered SQL,
 * binds and locks stay byte-identical to the original statements. */
export const posScopeHeadSql = (): string => '(SELECT ?::text company_id,?::text branch_id,';
