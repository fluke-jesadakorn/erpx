/** Procurement read-tail r47 shared fragment. Text only: each settlement fact
 * keeps its verbatim head and source; only the outer head/FROM wrapper is
 * delegated. Kind labels, `::text` casts, pending booleans, UNION ALL
 * separators, the scope CTE, binds and locks are unchanged. r17-r45 helpers stay untouched. */
export const procurementSettlementFactSelect = (head: string, source: string) =>
  `SELECT ${head} FROM ${source}`;
