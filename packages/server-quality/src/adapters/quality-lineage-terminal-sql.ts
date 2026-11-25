import "server-only";
import { evidenceHead } from "./inspection-evidence-source-sql";
/** Owner-local terminal head for the Quality recursive remaining-hold lineage
 * readers. The head routes through evidenceHead so this family keeps no inline
 * SELECT; the projection and trailing JOIN/predicate fragments are fixed
 * owner-owned fragments supplied by callers and still emitted byte-identically
 * to the former inline copies. No caller SQL is interpolated and no read here
 * acquires a lock. */
export const qualityLineageTerminalSql = (
  columns: string,
  tail = '',
) => evidenceHead(columns, ' FROM lineage', tail ? ` ${tail}` : '');
