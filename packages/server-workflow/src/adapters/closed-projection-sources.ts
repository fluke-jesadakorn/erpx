import 'server-only';
/** Byte-identical closed flow-routes projections; callers choose the exact comma separator. */
const flowRoutesColumns = ['"company_id"', '"kind"', '"record_id"', '"version"', '"defaults_revision"', '"steps_json"', '"created_at"'] as const;
export const flowRoutesClosedProjectionSql = (separator: string): string => `SELECT ${flowRoutesColumns.join(separator)} FROM flow_routes`;
/** Byte-identical command-receipts projections; callers choose columns, exact separator and trailing filter. */
export const commandReceiptsProjectionSql = (columns: readonly string[], separator = ',', tail = ''): string => `SELECT ${columns.join(separator)} FROM command_receipts${tail}`;
