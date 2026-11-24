import "server-only";
/** Owner-local command-receipt authority/result guard shared by the Quality
 * package-obligation facts. Every argument is a server-owned SQL expression or
 * a fixed operation/field literal; no bind value or caller SQL is interpolated.
 * Each emitted guard is byte-identical (modulo insignificant SQL whitespace) to
 * the former inline EXISTS subquery it replaces. */
export const qualityReceiptGuardSql = (
  record: string,
  operation: string,
  company: string,
  resultField: string,
  resultValue: string,
  actor?: string,
) => `SELECT 1 FROM command_receipts receipt WHERE receipt.record_id=${record} AND receipt.operation='${operation}'${actor ? ` AND receipt.actor_id=${actor}` : ""} AND receipt.authority_json::jsonb->>'companyId'=${company} AND receipt.result_json::jsonb->>'${resultField}'=${resultValue}`;
