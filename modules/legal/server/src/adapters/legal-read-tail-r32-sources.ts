import "server-only";
import { legalPointByKeySql } from './legal-read-tail-r36-sources';
/** Byte-identical Legal single-key point-read SQL shared by owner adapters (round 32).
 * R62: delegates to the verified r36 arbitrary-key renderer with the fixed 'id' key.
 * Text only: every call site still prepares, binds and executes its own read with
 * unchanged binds; the caller supplies the projection and fact table. No SQL inlined. */
export const legalPointByIdSql = (columns: string, table: string) =>
  legalPointByKeySql(columns, table, 'id');
