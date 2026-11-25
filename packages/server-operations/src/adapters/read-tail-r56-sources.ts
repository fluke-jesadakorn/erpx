import 'server-only';
/** Canonical owner-local bare SELECT-1-FROM head for the Operations read tail.
 *  The source (a table/alias pair or a materialized CTE name) is the only
 *  varying byte. Each rendering is byte-identical to the retired inline head;
 *  binds and locks stay local to each caller. */
export const OPERATIONS_SELECT_1_FROM = (source: string) => `SELECT 1 FROM ${source}`;
