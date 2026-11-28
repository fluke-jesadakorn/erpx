import "server-only";
import { evidenceHead } from "./inspection-evidence-source-sql";
/** Owner-local recursive remaining-hold lineage templates shared by the Quality
 * purchase-source readers. The zero-depth anchor seed and the depth+1 recursive
 * member are emitted byte-identically to the former inline copies when callers
 * supply the same fixed owner-owned column lists, source and row alias. No
 * caller SQL is interpolated and no read acquires a lock. */
/** One byte-identical head shared by the anchor seed and the recursive member:
 * both opened with the same projected-column prefix before the r32 fold. Each
 * caller supplies only fixed owner-owned fragments; no bind or lock is added. */
const lineageHead = (columns: string, tail: string) => evidenceHead(`${columns},`, "", tail);
export const qualityLineageSeedSql = (
  columns: string,
  source: string,
) => lineageHead(columns, `0 depth FROM ${source}`);
export const qualityLineageStepSql = (
  columns: string,
  alias: string,
) => lineageHead(columns, `${alias}.depth+1 FROM lineage ${alias}`);
