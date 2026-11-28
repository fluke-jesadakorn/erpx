import 'server-only';
/** Agent-owned SELECT-column head shared by the catalog-list, company-settings,
 * turn-evidence and turn-by-id readers. Only the trailing FROM source varies;
 * each caller keeps its own columns, predicates and binds, and the native
 * owner remains the authority. */
export const agentSelectColsFromSql = (cols: string, tail: string) =>
  `SELECT ${cols} FROM ${tail}`;
