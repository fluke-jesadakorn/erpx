import "server-only";
import { legalPointByIdAndKeySql } from './legal-read-tail-r34-sources';
/** Byte-identical Legal company-scoped point-read SQL shared by owner adapters (round 28).
 * Delegates to the verified r34 id+key head; the caller supplies only the fixed
 * legal fact table. No bind is added and every rendered statement stays exact. */
/** Fixed-table company-scoped point read; the caller supplies the fixed legal fact table. */
export const legalByIdAndCompanySql = (table: string) =>
  legalPointByIdAndKeySql('*', table, 'company_id');
