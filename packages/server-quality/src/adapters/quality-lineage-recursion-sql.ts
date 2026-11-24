import "server-only";
/** Owner-local recursive remaining-hold lineage templates shared by the Quality
 * purchase-source readers. The zero-depth anchor seed and the depth+1 recursive
 * member are emitted byte-identically to the former inline copies when callers
 * supply the same fixed owner-owned column lists, source and row alias. No
 * caller SQL is interpolated and no read acquires a lock. */
export const qualityLineageSeedSql = (
  columns: string,
  source: string,
) => `SELECT ${columns},0 depth FROM ${source}`;
export const qualityLineageStepSql = (
  columns: string,
  alias: string,
) => `SELECT ${columns},${alias}.depth+1 FROM lineage ${alias}`;
