import 'server-only';
/** Byte-identical bare head shared by the closed projection renderers; callers keep their exact column, separator and tail fragments. */
export const selectHeadSql = 'SELECT ';
/** Byte-identical closed flow-routes projections; callers choose the exact comma separator. */
const flowRoutesColumns = ['"company_id"', '"kind"', '"record_id"', '"version"', '"defaults_revision"', '"steps_json"', '"created_at"'] as const;
export const flowRoutesClosedProjectionSql = (separator: string): string => `${selectHeadSql}${flowRoutesColumns.join(separator)} FROM flow_routes`;
/** Byte-identical command-receipts projections; callers choose columns, exact separator and trailing filter. */
export const commandReceiptsProjectionSql = (columns: readonly string[], separator = ',', tail = ''): string => `${selectHeadSql}${columns.join(separator)} FROM command_receipts${tail}`;
/** Byte-identical table-scoped EXISTS head shared by the approval and flow version guards; callers keep their exact WHERE tail and bind order. */
export const tableExistsGuardSql = (table: string, tail: string): string => `EXISTS(SELECT 1 FROM ${table} WHERE ${tail}`;
