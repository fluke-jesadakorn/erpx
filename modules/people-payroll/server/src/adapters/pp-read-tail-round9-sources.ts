import 'server-only';
/** Canonical owner renderer for the quoted projection fragments shared between
 * JOIN and FROM forms. Callers pin the exact quoted column list, table and alias;
 * the rendered fragment is byte-identical to the former inline literal. Pure read
 * projection only: no writes, row/table locks, advisory statements, native kinds
 * or cross-owner projection changes. */

export const ppRdRound9ProjectionFragment = (
  keyword: 'JOIN' | 'FROM',
  columns: string,
  table: string,
  alias: string,
) => `${keyword} (SELECT ${columns} FROM ${table}) ${alias}`;
